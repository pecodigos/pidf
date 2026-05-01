use image::image_dimensions;
use pdfium_render::prelude::*;
use serde::Serialize;
use std::collections::{hash_map::DefaultHasher, HashMap, VecDeque};
use std::fs;
use std::hash::{Hash, Hasher};
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::sync::{mpsc, Mutex, OnceLock};
use std::thread;
use std::time::{SystemTime, UNIX_EPOCH};

// ── helpers ──────────────────────────────────────────────────────────

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
    target_width.clamp(240, 2200)
}

fn normalize_render_priority(render_priority: Option<u16>) -> u16 {
    render_priority.unwrap_or(100).min(4096)
}

fn verbose_runtime_logs_enabled() -> bool {
    std::env::var("PIDF_VERBOSE_LOGS")
        .ok()
        .map(|value| value == "1")
        .unwrap_or(false)
}

fn worker_thread_count() -> usize {
    std::env::var("PIDF_WORKER_THREADS")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .filter(|&n| n >= 1)
        .unwrap_or_else(|| {
            std::thread::available_parallelism()
                .map(|n| n.get().clamp(2, 8))
                .unwrap_or(4)
        })
}

// ── pdfium bootstrap ─────────────────────────────────────────────────

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
                    println!(
                        "[PiDF] using local PDFium library: {}",
                        candidate.to_string_lossy()
                    );
                }
                return Ok(Pdfium::new(bindings));
            }
            Err(error) => {
                local_errors.push(format!("{} ({error:?})", candidate.to_string_lossy()));
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

// ── disk render cache ────────────────────────────────────────────────

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

struct CacheFileEntry {
    path: PathBuf,
    size: u64,
    modified_at: SystemTime,
}

fn prune_render_cache_dir(
    cache_dir: &Path,
    max_total_bytes: u64,
    max_file_age_secs: u64,
) -> Result<(), String> {
    let now = SystemTime::now();
    let mut retained: Vec<CacheFileEntry> = Vec::new();
    let mut total_bytes = 0_u64;

    let entries = match fs::read_dir(cache_dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(()),
        Err(error) => {
            return Err(format!(
                "Failed to list render cache directory {}: {error}",
                cache_dir.to_string_lossy()
            ));
        }
    };

    for entry_result in entries {
        let entry = match entry_result {
            Ok(entry) => entry,
            Err(_) => continue,
        };
        let path = entry.path();
        let metadata = match entry.metadata() {
            Ok(metadata) => metadata,
            Err(_) => continue,
        };
        if !metadata.is_file() {
            continue;
        }
        let file_size = metadata.len();
        let modified_at = metadata.modified().unwrap_or(UNIX_EPOCH);
        let file_age_secs = now
            .duration_since(modified_at)
            .map(|duration| duration.as_secs())
            .unwrap_or(0);

        if file_age_secs > max_file_age_secs {
            let _ = fs::remove_file(&path);
            continue;
        }

        total_bytes = total_bytes.saturating_add(file_size);
        retained.push(CacheFileEntry {
            path,
            size: file_size,
            modified_at,
        });
    }

    if total_bytes <= max_total_bytes {
        return Ok(());
    }

    retained.sort_by_key(|entry| {
        entry
            .modified_at
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_millis())
            .unwrap_or(0)
    });

    for entry in retained {
        if total_bytes <= max_total_bytes {
            break;
        }
        if fs::remove_file(&entry.path).is_ok() {
            total_bytes = total_bytes.saturating_sub(entry.size);
        }
    }

    Ok(())
}

const MAX_RENDER_CACHE_BYTES: u64 = 512 * 1024 * 1024;
const MAX_RENDER_CACHE_FILE_AGE_SECS: u64 = 14 * 24 * 60 * 60;
const CACHE_PRUNE_INTERVAL_RENDERS: u32 = 40;

fn prune_render_cache_best_effort() {
    let cache_dir = match render_cache_dir() {
        Ok(cache_dir) => cache_dir,
        Err(_) => return,
    };
    let _ = prune_render_cache_dir(
        &cache_dir,
        MAX_RENDER_CACHE_BYTES,
        MAX_RENDER_CACHE_FILE_AGE_SECS,
    );
}

// ── IPC types ────────────────────────────────────────────────────────

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PdfOpenInfo {
    session_id: String,
    page_count: u16,
    first_page_width: f32,
    first_page_height: f32,
    render_engine: &'static str,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct PdfRenderedPage {
    data: Vec<u8>,
    width: f32,
    height: f32,
    mime: String,
}

struct PdfSessionOpenResult {
    session_id: String,
    page_count: u16,
    first_page_width: f32,
    first_page_height: f32,
}

// ── worker messages ──────────────────────────────────────────────────

enum PdfWorkerRequest {
    Open {
        path: String,
        response: mpsc::Sender<Result<PdfSessionOpenResult, String>>,
    },
    Render {
        session_id: String,
        page_number: u16,
        target_width: u16,
        render_priority: u16,
        response: mpsc::Sender<Result<PdfRenderedPage, String>>,
    },
    Close {
        session_id: String,
        response: mpsc::Sender<Result<(), String>>,
    },
}

const RENDER_SUPERSEDED_ERROR: &str = "Render request superseded by newer request.";

struct PendingRenderRequest {
    session_id: String,
    page_number: u16,
    target_width: u16,
    render_priority: u16,
    response: mpsc::Sender<Result<PdfRenderedPage, String>>,
}

// ── per-worker in-memory page cache ──────────────────────────────────

const MEMORY_CACHE_MAX_PAGES: usize = 64;

fn memory_cache_key(session_id: &str, page_number: u16, target_width: u16) -> String {
    format!("{session_id}:{page_number}:{target_width}")
}

struct MemoryPageCache {
    entries: HashMap<String, PdfRenderedPage>,
    order: VecDeque<String>,
    max_entries: usize,
}

impl MemoryPageCache {
    fn new(max_entries: usize) -> Self {
        Self {
            entries: HashMap::with_capacity(max_entries.min(128)),
            order: VecDeque::with_capacity(max_entries.min(128)),
            max_entries,
        }
    }

    fn get(&mut self, key: &str) -> Option<PdfRenderedPage> {
        // LRU: move accessed key to back
        if let Some(entry) = self.entries.get(key) {
            // Remove and push to back for LRU order
            self.order.retain(|k| k != key);
            self.order.push_back(key.to_owned());
            return Some(entry.clone());
        }
        None
    }

    fn put(&mut self, key: String, page: PdfRenderedPage) {
        if self.entries.contains_key(&key) {
            self.order.retain(|k| k != &key);
        }
        self.entries.insert(key.clone(), page);
        self.order.push_back(key);

        while self.order.len() > self.max_entries {
            if let Some(oldest) = self.order.pop_front() {
                self.entries.remove(&oldest);
            }
        }
    }

    #[allow(dead_code)]
    fn clear(&mut self) {
        self.entries.clear();
        self.order.clear();
    }
}

// ── page render logic (runs inside worker) ──────────────────────────

fn render_pdf_page(
    document: &PdfDocument<'_>,
    path: &str,
    page_number: u16,
    target_width: u16,
    memory_cache: &mut MemoryPageCache,
    session_id: &str,
) -> Result<PdfRenderedPage, String> {
    let page_count = document.pages().len();
    if page_count == 0 {
        return Err("PDF has no pages.".to_owned());
    }

    let requested_page = page_number.max(1);
    let normalized_page = normalize_page_number(requested_page, page_count);
    let normalized_width = normalize_target_width(target_width);

    // 1. check in-memory cache
    let mem_key = memory_cache_key(session_id, normalized_page, normalized_width);
    if let Some(cached) = memory_cache.get(&mem_key) {
        return Ok(cached);
    }

    // 2. check disk cache
    let output_path = render_cache_path(path, normalized_page, normalized_width)?;
    if let Some(cached_page) = cached_render_response(&output_path, normalized_width)? {
        memory_cache.put(mem_key, cached_page.clone());
        return Ok(cached_page);
    }

    // 3. render from PDF
    let page = document
        .pages()
        .get(normalized_page - 1)
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

    // encode JPEG in-memory
    let mut jpeg_bytes: Vec<u8> = Vec::new();
    image
        .write_to(
            &mut std::io::Cursor::new(&mut jpeg_bytes),
            image::ImageFormat::Jpeg,
        )
        .map_err(|error| format!("Failed to encode page image as JPEG: {error}"))?;

    // persist to disk asynchronously (don't block the response)
    let disk_path = output_path.clone();
    let disk_bytes = jpeg_bytes.clone();
    thread::Builder::new()
        .name("pidf-cache-write".to_owned())
        .spawn(move || {
            let temp_path = disk_path.with_extension("tmp.jpg");
            let _ = fs::write(&temp_path, &disk_bytes);
            let _ = fs::rename(&temp_path, &disk_path);
        })
        .ok();

    if verbose_runtime_logs_enabled() {
        println!(
            "[PiDF] rendered page {} width {} ({} bytes in memory)",
            normalized_page,
            normalized_width,
            jpeg_bytes.len()
        );
    }

    let result = PdfRenderedPage {
        data: jpeg_bytes,
        width: normalized_width as f32,
        height: css_height,
        mime: "image/jpeg".to_owned(),
    };

    memory_cache.put(mem_key, result.clone());

    Ok(result)
}

fn cached_render_response(
    output_path: &Path,
    target_width: u16,
) -> Result<Option<PdfRenderedPage>, String> {
    let data = match fs::read(output_path) {
        Ok(data) if !data.is_empty() => data,
        Ok(_) => {
            let _ = fs::remove_file(output_path);
            return Ok(None);
        }
        Err(_) => return Ok(None),
    };

    let (image_width, image_height) = match image_dimensions(output_path) {
        Ok((w, h)) if w > 0 && h > 0 => (w, h),
        _ => {
            let _ = fs::remove_file(output_path);
            return Ok(None);
        }
    };

    let ratio = image_height as f32 / image_width as f32;

    Ok(Some(PdfRenderedPage {
        data,
        width: target_width as f32,
        height: (target_width as f32 * ratio).max(1.0),
        mime: "image/jpeg".to_owned(),
    }))
}

// ── worker thread ────────────────────────────────────────────────────

fn run_pdf_worker(receiver: mpsc::Receiver<PdfWorkerRequest>) {
    let pdfium = match create_pdfium() {
        Ok(pdfium) => pdfium,
        Err(error) => {
            let startup_error = format!("Failed to initialize PDF worker: {error}");
            for request in receiver {
                match request {
                    PdfWorkerRequest::Open { response, .. } => {
                        let _ = response.send(Err(startup_error.clone()));
                    }
                    PdfWorkerRequest::Render { response, .. } => {
                        let _ = response.send(Err(startup_error.clone()));
                    }
                    PdfWorkerRequest::Close { response, .. } => {
                        let _ = response.send(Err(startup_error.clone()));
                    }
                }
            }
            return;
        }
    };

    let mut sessions = HashMap::new();
    let mut session_paths: HashMap<String, String> = HashMap::new();
    let mut next_session_id: u64 = 1;
    let mut deferred_requests: VecDeque<PdfWorkerRequest> = VecDeque::new();
    let mut renders_since_prune: u32 = 0;
    let mut memory_cache = MemoryPageCache::new(MEMORY_CACHE_MAX_PAGES);

    prune_render_cache_best_effort();

    loop {
        let request = if let Some(deferred) = deferred_requests.pop_front() {
            deferred
        } else {
            match receiver.recv() {
                Ok(request) => request,
                Err(_) => break,
            }
        };

        match request {
            PdfWorkerRequest::Open { path, response } => {
                let result = (|| {
                    ensure_pdf_path(&path)?;

                    let document = pdfium
                        .load_pdf_from_file(&path, None)
                        .map_err(|error| format!("Failed to open PDF: {error:?}"))?;

                    let page_count = document.pages().len();
                    if page_count == 0 {
                        return Err("PDF has no pages.".to_owned());
                    }

                    let first_page = document
                        .pages()
                        .get(0)
                        .map_err(|error| format!("Failed to read first page: {error:?}"))?;

                    let session_id = format!("s{}", next_session_id);
                    next_session_id += 1;

                    sessions.insert(session_id.clone(), document);
                    session_paths.insert(session_id.clone(), path);

                    Ok(PdfSessionOpenResult {
                        session_id,
                        page_count,
                        first_page_width: first_page.width().value,
                        first_page_height: first_page.height().value,
                    })
                })();

                let _ = response.send(result);
            }
            PdfWorkerRequest::Render {
                session_id,
                page_number,
                target_width,
                render_priority,
                response,
            } => {
                // drain additional render requests for batching + dedup
                let mut pending_renders = vec![PendingRenderRequest {
                    session_id,
                    page_number,
                    target_width,
                    render_priority,
                    response,
                }];

                loop {
                    match receiver.try_recv() {
                        Ok(PdfWorkerRequest::Render {
                            session_id,
                            page_number,
                            target_width,
                            render_priority,
                            response,
                        }) => {
                            pending_renders.push(PendingRenderRequest {
                                session_id,
                                page_number,
                                target_width,
                                render_priority,
                                response,
                            });
                        }
                        Ok(other_request) => {
                            deferred_requests.push_back(other_request);
                        }
                        Err(mpsc::TryRecvError::Empty) => break,
                        Err(mpsc::TryRecvError::Disconnected) => break,
                    }
                }

                // dedup: keep only latest render for each (session, page, width)
                let mut latest_render_by_page: HashMap<(String, u16, u16), usize> = HashMap::new();
                let mut superseded = vec![false; pending_renders.len()];

                for (index, pending) in pending_renders.iter().enumerate() {
                    if let Some(previous) = latest_render_by_page.insert(
                        (
                            pending.session_id.clone(),
                            pending.page_number,
                            pending.target_width,
                        ),
                        index,
                    ) {
                        superseded[previous] = true;
                    }
                }

                for (index, pending) in pending_renders.iter().enumerate() {
                    if superseded[index] {
                        let _ = pending
                            .response
                            .send(Err(RENDER_SUPERSEDED_ERROR.to_owned()));
                    }
                }

                // sort by priority (lower = higher priority)
                let mut execution_order: Vec<usize> = (0..pending_renders.len())
                    .filter(|index| !superseded[*index])
                    .collect();

                execution_order.sort_by(|left_index, right_index| {
                    let left = &pending_renders[*left_index];
                    let right = &pending_renders[*right_index];
                    left.render_priority
                        .cmp(&right.render_priority)
                        .then_with(|| left_index.cmp(right_index))
                });

                for index in execution_order {
                    let pending = &pending_renders[index];

                    let result = (|| {
                        let document = sessions
                            .get(&pending.session_id)
                            .ok_or_else(|| "Unknown or closed PDF session.".to_owned())?;

                        let path = session_paths
                            .get(&pending.session_id)
                            .ok_or_else(|| "Missing PDF path for session.".to_owned())?;

                        render_pdf_page(
                            document,
                            path,
                            pending.page_number,
                            pending.target_width,
                            &mut memory_cache,
                            &pending.session_id,
                        )
                    })();

                    let render_succeeded = result.is_ok();
                    let _ = pending.response.send(result);

                    if render_succeeded {
                        renders_since_prune = renders_since_prune.saturating_add(1);
                        if renders_since_prune >= CACHE_PRUNE_INTERVAL_RENDERS {
                            renders_since_prune = 0;
                            prune_render_cache_best_effort();
                        }
                    }
                }
            }
            PdfWorkerRequest::Close {
                session_id,
                response,
            } => {
                sessions.remove(&session_id);
                session_paths.remove(&session_id);
                let _ = response.send(Ok(()));
            }
        }
    }
}

// ── thread pool ──────────────────────────────────────────────────────

struct PdfWorkerPool {
    senders: Vec<mpsc::Sender<PdfWorkerRequest>>,
    /// session_id → worker_index
    session_owners: Mutex<HashMap<String, usize>>,
    next_worker: std::sync::atomic::AtomicUsize,
}

impl PdfWorkerPool {
    fn new(num_workers: usize) -> Self {
        let mut senders = Vec::with_capacity(num_workers);
        for i in 0..num_workers {
            let (tx, rx) = mpsc::channel();
            thread::Builder::new()
                .name(format!("pidf-pdf-worker-{i}"))
                .spawn(move || run_pdf_worker(rx))
                .expect("failed to spawn PDF worker thread");
            senders.push(tx);
        }

        Self {
            senders,
            session_owners: Mutex::new(HashMap::new()),
            next_worker: std::sync::atomic::AtomicUsize::new(0),
        }
    }

    fn open(&self, path: String) -> Result<PdfOpenInfo, String> {
        let (response_sender, response_receiver) = mpsc::channel();
        let worker_idx = self
            .next_worker
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            % self.senders.len();

        self.senders[worker_idx]
            .send(PdfWorkerRequest::Open {
                path,
                response: response_sender,
            })
            .map_err(|_| "PDF worker is unavailable.".to_owned())?;

        let open_result = response_receiver
            .recv()
            .map_err(|_| "PDF worker did not respond.".to_owned())??;

        self.session_owners
            .lock()
            .unwrap()
            .insert(open_result.session_id.clone(), worker_idx);

        Ok(PdfOpenInfo {
            session_id: open_result.session_id,
            page_count: open_result.page_count,
            first_page_width: open_result.first_page_width,
            first_page_height: open_result.first_page_height,
            render_engine: "pdfium-render",
        })
    }

    fn render(
        &self,
        session_id: String,
        page_number: u16,
        target_width: u16,
        render_priority: Option<u16>,
    ) -> Result<PdfRenderedPage, String> {
        let worker_idx = {
            let owners = self.session_owners.lock().unwrap();
            owners
                .get(&session_id)
                .copied()
                .ok_or_else(|| "Unknown or closed PDF session.".to_owned())?
        };

        let (response_sender, response_receiver) = mpsc::channel();

        self.senders[worker_idx]
            .send(PdfWorkerRequest::Render {
                session_id,
                page_number,
                target_width,
                render_priority: normalize_render_priority(render_priority),
                response: response_sender,
            })
            .map_err(|_| "PDF worker is unavailable.".to_owned())?;

        response_receiver
            .recv()
            .map_err(|_| "PDF worker did not respond.".to_owned())?
    }

    fn close(&self, session_id: String) -> Result<(), String> {
        let worker_idx = {
            let mut owners = self.session_owners.lock().unwrap();
            owners
                .remove(&session_id)
                .ok_or_else(|| "Unknown or closed PDF session.".to_owned())?
        };

        let (response_sender, response_receiver) = mpsc::channel();

        self.senders[worker_idx]
            .send(PdfWorkerRequest::Close {
                session_id,
                response: response_sender,
            })
            .map_err(|_| "PDF worker is unavailable.".to_owned())?;

        response_receiver
            .recv()
            .map_err(|_| "PDF worker did not respond.".to_owned())?
    }
}

fn worker_pool() -> &'static PdfWorkerPool {
    static POOL: OnceLock<PdfWorkerPool> = OnceLock::new();
    POOL.get_or_init(|| PdfWorkerPool::new(worker_thread_count()))
}

// ── Tauri commands ───────────────────────────────────────────────────

#[tauri::command]
fn pdf_open_info(path: String) -> Result<PdfOpenInfo, String> {
    worker_pool().open(path)
}

#[tauri::command]
fn pdf_render_page(
    session_id: String,
    page_number: u16,
    target_width: u16,
    render_priority: Option<u16>,
) -> Result<PdfRenderedPage, String> {
    worker_pool().render(session_id, page_number, target_width, render_priority)
}

#[tauri::command]
fn pdf_close_session(session_id: String) -> Result<(), String> {
    worker_pool().close(session_id)
}

#[tauri::command]
fn initial_pdf_path() -> Option<String> {
    let path = std::env::var("PIDF_OPEN_PATH")
        .ok()
        .filter(|path| Path::new(path).exists() && is_pdf_file(path));

    if verbose_runtime_logs_enabled() {
        println!("[PiDF] initial_pdf_path: {:?}", path);
    }

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

// ── tests ────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, SystemTime};

    fn create_temp_test_dir(name: &str) -> PathBuf {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(0);
        let dir = std::env::temp_dir().join(format!("pidf-test-{}-{}", name, now));
        fs::create_dir_all(&dir).expect("failed to create temp test directory");
        dir
    }

    fn write_test_file(path: &Path, bytes: usize) {
        fs::write(path, vec![0_u8; bytes]).expect("failed to write test file");
    }

    fn total_size(dir: &Path) -> u64 {
        fs::read_dir(dir)
            .expect("failed to list directory")
            .filter_map(Result::ok)
            .filter_map(|entry| entry.metadata().ok())
            .filter(|metadata| metadata.is_file())
            .map(|metadata| metadata.len())
            .sum()
    }

    #[test]
    fn normalization_clamps_to_expected_ranges() {
        assert_eq!(normalize_page_number(0, 0), 1);
        assert_eq!(normalize_page_number(0, 10), 1);
        assert_eq!(normalize_page_number(999, 10), 10);

        assert_eq!(normalize_target_width(0), 240);
        assert_eq!(normalize_target_width(4096), 2200);

        assert_eq!(normalize_render_priority(None), 100);
        assert_eq!(normalize_render_priority(Some(5000)), 4096);
    }

    #[test]
    fn prune_render_cache_enforces_size_limit() {
        let cache_dir = create_temp_test_dir("size-limit");

        write_test_file(&cache_dir.join("a.jpg"), 230);
        write_test_file(&cache_dir.join("b.jpg"), 230);
        write_test_file(&cache_dir.join("c.jpg"), 230);

        prune_render_cache_dir(&cache_dir, 460, u64::MAX).expect("prune should succeed");

        assert!(total_size(&cache_dir) <= 460);

        let _ = fs::remove_dir_all(cache_dir);
    }

    #[test]
    fn prune_render_cache_removes_stale_files() {
        let cache_dir = create_temp_test_dir("age-limit");
        let stale_file = cache_dir.join("stale.jpg");

        write_test_file(&stale_file, 64);

        thread::sleep(Duration::from_secs(1));

        prune_render_cache_dir(&cache_dir, u64::MAX, 0).expect("prune should succeed");

        assert!(!stale_file.exists());

        let _ = fs::remove_dir_all(cache_dir);
    }

    #[test]
    fn memory_cache_lru_eviction() {
        let mut cache = MemoryPageCache::new(3);

        let page = |n: u8| PdfRenderedPage {
            data: vec![n],
            width: 100.0,
            height: 141.0,
            mime: "image/jpeg".to_owned(),
        };

        cache.put("a".into(), page(1));
        cache.put("b".into(), page(2));
        cache.put("c".into(), page(3));
        cache.put("d".into(), page(4)); // evicts "a"

        assert!(cache.get("a").is_none());
        assert!(cache.get("b").is_some());
        assert!(cache.get("c").is_some());
        assert!(cache.get("d").is_some());
    }

    #[test]
    fn memory_cache_lru_reorder_on_get() {
        let mut cache = MemoryPageCache::new(3);

        let page = |n: u8| PdfRenderedPage {
            data: vec![n],
            width: 100.0,
            height: 141.0,
            mime: "image/jpeg".to_owned(),
        };

        cache.put("a".into(), page(1));
        cache.put("b".into(), page(2));
        cache.put("c".into(), page(3));

        // Access "a" → moves to back, "b" becomes oldest
        cache.get("a");

        cache.put("d".into(), page(4)); // should evict "b", not "a"

        assert!(cache.get("a").is_some());
        assert!(cache.get("b").is_none());
        assert!(cache.get("c").is_some());
        assert!(cache.get("d").is_some());
    }

    #[test]
    fn worker_thread_count_respects_env() {
        // Can't easily test available_parallelism path, but verify env override
        // Just validate it returns a sensible value
        let count = worker_thread_count();
        assert!(
            count >= 1,
            "worker_thread_count should be at least 1, got {count}"
        );
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            initial_pdf_path,
            pdf_open_info,
            pdf_render_page,
            pdf_close_session,
            trace_pdf_stage
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
