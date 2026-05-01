export function extractFileName(path: string): string {
  if (!path) {
    return "";
  }

  const parts = path.split(/[\\/]/);
  return parts[parts.length - 1] || path;
}
