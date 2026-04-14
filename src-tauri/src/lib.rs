use image::image_dimensions;
use pdfium_render::prelude::*;
use serde::Serialize;
use std::collections::hash_map::DefaultHasher;
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::UNIX_EPOCH;

fn is_pdf_file(path: &str) -> bool {
    let file_path = Path::new(path);
    file_path
        .extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| extension.eq_ignore_ascii_case("pdf"))
        .unwrap_or(false)
}

fn normalize_page_number(page_number: u16, page_count: u16) -> u16 {
    if page_count == 0 {
        return 1;
    }

    page_number.max(1).min(page_count)
}

fn normalize_target_width(target_width: u16) -> u16 {
    target_width.max(240).min(2200)
}

fn verbose_runtime_logs_enabled() -> bool {
    std::env::var("PIDF_VERBOSE_LOGS")
        .ok()
        .map(|value| value == "1")
        .unwrap_or(false)
}

fn render_lock() -> &'static Mutex<()> {
    static RENDER_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    RENDER_LOCK.get_or_init(|| Mutex::new(()))
}

fn local_pdfium_library_candidates() -> Vec<PathBuf> {
    let library_name = Pdfium::pdfium_platform_library_name();

    vec![
        PathBuf::from("pdfium").join("lib").join(&library_name),
        PathBuf::from("src-tauri")
            .join("pdfium")
            .join("lib")
            .join(&library_name),
        PathBuf::from("..")
            .join("src-tauri")
            .join("pdfium")
            .join("lib")
            .join(&library_name),
    ]
}

fn create_pdfium() -> Result<Pdfium, String> {
    let mut local_errors: Vec<String> = Vec::new();

    for candidate in local_pdfium_library_candidates() {
        if !candidate.exists() {
            continue;
        }

        match Pdfium::bind_to_library(&candidate) {
            Ok(bindings) => {
                if verbose_runtime_logs_enabled() {
                    println!("[PiDF] using local PDFium library: {}", candidate.to_string_lossy());
                }
                return Ok(Pdfium::new(bindings));
            }
            Err(error) => {
                local_errors.push(format!(
                    "{} ({error:?})",
                    candidate.to_string_lossy()
                ));
            }
        }
    }

    match Pdfium::bind_to_system_library() {
        Ok(bindings) => {
            if verbose_runtime_logs_enabled() {
                println!("[PiDF] using system PDFium library");
            }
            Ok(Pdfium::new(bindings))
        }
        Err(system_error) => {
            if local_errors.is_empty() {
                Err(format!(
                    "Failed to load system PDFium library: {system_error:?}"
                ))
            } else {
                Err(format!(
                    "Failed to load local PDFium library candidates [{}] and failed to load system PDFium library: {system_error:?}",
                    local_errors.join("; ")
                ))
            }
        }
    }
}

fn ensure_pdf_path(path: &str) -> Result<(), String> {
    if !Path::new(path).exists() {
        return Err("PDF path does not exist.".to_owned());
    }

    if !is_pdf_file(path) {
        return Err("Selected file is not a PDF.".to_owned());
    }

    Ok(())
}

fn render_cache_dir() -> Result<PathBuf, String> {
    let base = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    let dir = base.join("pidf-render-cache");
    fs::create_dir_all(&dir)
        .map_err(|error| format!("Failed to create render cache directory: {error}"))?;
    Ok(dir)
}

fn cache_stamp(path: &str) -> u64 {
    fs::metadata(path)
        .ok()
        .and_then(|metadata| metadata.modified().ok())
        .and_then(|modified| modified.duration_since(UNIX_EPOCH).ok())
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

fn render_cache_path(path: &str, page_number: u16, target_width: u16) -> Result<PathBuf, String> {
    let mut hasher = DefaultHasher::new();
    path.hash(&mut hasher);
    cache_stamp(path).hash(&mut hasher);
    page_number.hash(&mut hasher);
    target_width.hash(&mut hasher);
    let key = hasher.finish();

    let file_name = format!("p{}_w{}_{}.jpg", page_number, target_width, key);
    Ok(render_cache_dir()?.join(file_name))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PdfOpenInfo {
    page_count: u16,
    first_page_width: f32,
    first_page_height: f32,
    render_engine: &'static str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PdfRenderedPage {
    image_path: String,
    width: f32,
    height: f32,
}

fn cached_render_response(output_path: &Path, target_width: u16) -> Result<Option<PdfRenderedPage>, String> {
    let metadata = match fs::metadata(output_path) {
        Ok(metadata) => metadata,
        Err(_) => return Ok(None),
    };

    if metadata.len() == 0 {
        let _ = fs::remove_file(output_path);
        return Ok(None);
    }

    match image_dimensions(output_path) {
        Ok((image_width, image_height)) if image_width > 0 && image_height > 0 => {
            let ratio = image_height as f32 / image_width as f32;

            Ok(Some(PdfRenderedPage {
                image_path: output_path.to_string_lossy().into_owned(),
                width: target_width as f32,
                height: (target_width as f32 * ratio).max(1.0),
            }))
        }
        _ => {
            let _ = fs::remove_file(output_path);
            Ok(None)
        }
    }
}

#[tauri::command]
fn pdf_open_info(path: String) -> Result<PdfOpenInfo, String> {
    ensure_pdf_path(&path)?;

    let pdfium = create_pdfium()?;
    let document = pdfium
        .load_pdf_from_file(&path, None)
        .map_err(|error| format!("Failed to open PDF: {error:?}"))?;

    let page_count = document.pages().len() as u16;
    if page_count == 0 {
        return Err("PDF has no pages.".to_owned());
    }

    let first_page = document
        .pages()
        .get(0)
        .map_err(|error| format!("Failed to read first page: {error:?}"))?;

    Ok(PdfOpenInfo {
        page_count,
        first_page_width: first_page.width().value,
        first_page_height: first_page.height().value,
        render_engine: "pdfium-render",
    })
}

#[tauri::command]
fn pdf_render_page(path: String, page_number: u16, target_width: u16) -> Result<PdfRenderedPage, String> {
    ensure_pdf_path(&path)?;

    let requested_page = page_number.max(1);
    let normalized_width = normalize_target_width(target_width);
    let mut output_path = render_cache_path(&path, requested_page, normalized_width)?;

    if let Some(cached_page) = cached_render_response(&output_path, normalized_width)? {
        return Ok(cached_page);
    }

    let _guard = render_lock()
        .lock()
        .map_err(|_| "Render lock poisoned.".to_owned())?;

    if let Some(cached_page) = cached_render_response(&output_path, normalized_width)? {
        return Ok(cached_page);
    }

    let pdfium = create_pdfium()?;
    let document = pdfium
        .load_pdf_from_file(&path, None)
        .map_err(|error| format!("Failed to open PDF for rendering: {error:?}"))?;

    let page_count = document.pages().len() as u16;
    if page_count == 0 {
        return Err("PDF has no pages.".to_owned());
    }

    let normalized_page = normalize_page_number(requested_page, page_count);

    if normalized_page != requested_page {
        output_path = render_cache_path(&path, normalized_page, normalized_width)?;

        if let Some(cached_page) = cached_render_response(&output_path, normalized_width)? {
            return Ok(cached_page);
        }
    }

    let page = document
        .pages()
        .get((normalized_page - 1) as u16)
        .map_err(|error| format!("Failed to access page {}: {error:?}", normalized_page))?;

    let base_width = page.width().value.max(1.0);
    let base_height = page.height().value.max(1.0);
    let scale = normalized_width as f32 / base_width;
    let css_height = (base_height * scale).max(1.0);

    let bitmap = page
        .render_with_config(
            &PdfRenderConfig::new()
                .set_target_width(normalized_width as i32)
                .rotate_if_landscape(PdfPageRenderRotation::None, true),
        )
        .map_err(|error| format!("Failed to render page {}: {error:?}", normalized_page))?;

    let image = bitmap.as_image();
    let temp_output_path = output_path.with_extension("tmp.jpg");

    image
        .save(&temp_output_path)
        .map_err(|error| format!("Failed to persist rendered page image: {error}"))?;

    fs::rename(&temp_output_path, &output_path)
        .map_err(|error| format!("Failed to finalize rendered page image: {error}"))?;

    let verbose_render = std::env::var("PIDF_VERBOSE_RENDER")
        .ok()
        .map(|value| value == "1")
        .unwrap_or(false);

    if verbose_render {
        let output_size = fs::metadata(&output_path)
            .map(|metadata| metadata.len())
            .unwrap_or(0);

        println!(
            "[PiDF] rendered page {} width {} -> {} ({} bytes)",
            normalized_page,
            normalized_width,
            output_path.to_string_lossy(),
            output_size
        );
    }

    Ok(PdfRenderedPage {
        image_path: output_path.to_string_lossy().into_owned(),
        width: normalized_width as f32,
        height: css_height,
    })
}

#[tauri::command]
fn initial_pdf_path() -> Option<String> {
    let path = std::env::var("PIDF_OPEN_PATH")
        .ok()
        .filter(|path| Path::new(path).exists() && is_pdf_file(path));

    println!("[PiDF] initial_pdf_path: {:?}", path);
    path
}

#[tauri::command]
fn trace_pdf_stage(
    stage: String,
    timestamp: String,
    elapsed_ms: Option<u64>,
    details_json: Option<String>,
) {
    let elapsed = elapsed_ms
        .map(|value| value.to_string())
        .unwrap_or_else(|| "n/a".to_owned());
    let details = details_json.unwrap_or_else(|| "{}".to_owned());

    println!(
        "[PiDF][stage] ts={} elapsed_ms={} stage={} details={}",
        timestamp, elapsed, stage, details
    );
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
    .plugin(tauri_plugin_dialog::init())
    .invoke_handler(tauri::generate_handler![
        initial_pdf_path,
        pdf_open_info,
        pdf_render_page,
        trace_pdf_stage
    ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
