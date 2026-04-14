import { describe, expect, it } from "vitest";

import {
  isEditableTarget,
  isEndShortcut,
  isFindShortcut,
  isFullscreenShortcut,
  isHomeShortcut,
  isNextPageShortcut,
  isOpenShortcut,
  isPageDownShortcut,
  isPageUpShortcut,
  isPreviousPageShortcut,
  isViewerZoomInShortcut,
  isViewerZoomOutShortcut,
  isZoomInShortcut,
  isZoomOutShortcut,
  isZoomResetShortcut,
} from "./keyboard";

type ShortcutTestEvent = {
  key: string;
  ctrlKey?: boolean;
  metaKey?: boolean;
  shiftKey?: boolean;
  altKey?: boolean;
};

function keyEvent(overrides: ShortcutTestEvent): KeyboardEvent {
  return {
    key: overrides.key,
    ctrlKey: overrides.ctrlKey ?? false,
    metaKey: overrides.metaKey ?? false,
    shiftKey: overrides.shiftKey ?? false,
    altKey: overrides.altKey ?? false,
  } as KeyboardEvent;
}

describe("keyboard shortcut helpers", () => {
  it("matches global primary shortcuts", () => {
    expect(isOpenShortcut(keyEvent({ key: "o", ctrlKey: true }))).toBe(true);
    expect(isFindShortcut(keyEvent({ key: "f", metaKey: true }))).toBe(true);
    expect(isZoomInShortcut(keyEvent({ key: "+", ctrlKey: true }))).toBe(true);
    expect(isZoomOutShortcut(keyEvent({ key: "-", metaKey: true }))).toBe(true);
    expect(isZoomResetShortcut(keyEvent({ key: "0", ctrlKey: true }))).toBe(true);
  });

  it("matches navigation shortcuts", () => {
    expect(isPreviousPageShortcut(keyEvent({ key: "ArrowLeft" }))).toBe(true);
    expect(isNextPageShortcut(keyEvent({ key: "ArrowRight" }))).toBe(true);
    expect(isPageUpShortcut(keyEvent({ key: "PageUp" }))).toBe(true);
    expect(isPageDownShortcut(keyEvent({ key: "PageDown" }))).toBe(true);
    expect(isHomeShortcut(keyEvent({ key: "Home" }))).toBe(true);
    expect(isEndShortcut(keyEvent({ key: "End" }))).toBe(true);
  });

  it("matches fullscreen and viewer-local zoom shortcuts", () => {
    expect(isFullscreenShortcut(keyEvent({ key: "F11" }))).toBe(true);
    expect(isFullscreenShortcut(keyEvent({ key: "f", ctrlKey: true, shiftKey: true }))).toBe(
      true,
    );
    expect(isViewerZoomInShortcut(keyEvent({ key: "+" }))).toBe(true);
    expect(isViewerZoomOutShortcut(keyEvent({ key: "-" }))).toBe(true);
  });

  it("rejects shortcuts when modifiers conflict", () => {
    expect(isPreviousPageShortcut(keyEvent({ key: "ArrowLeft", altKey: true }))).toBe(false);
    expect(isOpenShortcut(keyEvent({ key: "o" }))).toBe(false);
    expect(isFindShortcut(keyEvent({ key: "f", ctrlKey: true, shiftKey: true }))).toBe(false);
  });

  it("detects editable targets safely", () => {
    expect(isEditableTarget({ tagName: "INPUT" })).toBe(true);
    expect(isEditableTarget({ tagName: "TEXTAREA" })).toBe(true);
    expect(isEditableTarget({ isContentEditable: true })).toBe(true);
    expect(isEditableTarget({ tagName: "DIV" })).toBe(false);
    expect(isEditableTarget(null)).toBe(false);
  });
});
