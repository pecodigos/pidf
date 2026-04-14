type KeyboardLikeEvent = Pick<
  KeyboardEvent,
  "key" | "ctrlKey" | "metaKey" | "shiftKey" | "altKey"
>;

type EditableTargetLike = {
  tagName?: unknown;
  isContentEditable?: unknown;
} | null;

function hasPrimaryModifier(event: KeyboardLikeEvent): boolean {
  return event.ctrlKey || event.metaKey;
}

function normalizedKey(event: KeyboardLikeEvent): string {
  return event.key.toLowerCase();
}

export function isEditableTarget(target: unknown): boolean {
  const element = target as EditableTargetLike;
  if (!element) {
    return false;
  }

  const tagName = typeof element.tagName === "string" ? element.tagName.toUpperCase() : "";
  return tagName === "INPUT" || tagName === "TEXTAREA" || Boolean(element.isContentEditable);
}

export function isOpenShortcut(event: KeyboardLikeEvent): boolean {
  return hasPrimaryModifier(event) && !event.shiftKey && normalizedKey(event) === "o";
}

export function isFindShortcut(event: KeyboardLikeEvent): boolean {
  return hasPrimaryModifier(event) && !event.shiftKey && normalizedKey(event) === "f";
}

export function isFullscreenShortcut(event: KeyboardLikeEvent): boolean {
  return event.key === "F11" || (hasPrimaryModifier(event) && event.shiftKey && normalizedKey(event) === "f");
}

export function isPreviousPageShortcut(event: KeyboardLikeEvent): boolean {
  return !event.altKey && !hasPrimaryModifier(event) && event.key === "ArrowLeft";
}

export function isNextPageShortcut(event: KeyboardLikeEvent): boolean {
  return !event.altKey && !hasPrimaryModifier(event) && event.key === "ArrowRight";
}

export function isZoomInShortcut(event: KeyboardLikeEvent): boolean {
  return hasPrimaryModifier(event) && (event.key === "+" || event.key === "=");
}

export function isZoomOutShortcut(event: KeyboardLikeEvent): boolean {
  return hasPrimaryModifier(event) && event.key === "-";
}

export function isZoomResetShortcut(event: KeyboardLikeEvent): boolean {
  return hasPrimaryModifier(event) && normalizedKey(event) === "0";
}

export function isViewerZoomInShortcut(event: KeyboardLikeEvent): boolean {
  return event.key === "+" || (hasPrimaryModifier(event) && event.key === "=");
}

export function isViewerZoomOutShortcut(event: KeyboardLikeEvent): boolean {
  return event.key === "-";
}

export function isPageDownShortcut(event: KeyboardLikeEvent): boolean {
  return event.key === "PageDown";
}

export function isPageUpShortcut(event: KeyboardLikeEvent): boolean {
  return event.key === "PageUp";
}

export function isHomeShortcut(event: KeyboardLikeEvent): boolean {
  return event.key === "Home";
}

export function isEndShortcut(event: KeyboardLikeEvent): boolean {
  return event.key === "End";
}
