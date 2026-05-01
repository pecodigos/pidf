import { describe, expect, it } from "vitest";

import { extractFileName } from "./path";

describe("path helpers", () => {
  it("extracts file names from unix and windows paths", () => {
    expect(extractFileName("/tmp/document.pdf")).toBe("document.pdf");
    expect(extractFileName("C:\\Users\\demo\\book.pdf")).toBe("book.pdf");
  });

  it("returns the original string when no separator exists", () => {
    expect(extractFileName("report.pdf")).toBe("report.pdf");
  });

  it("returns empty string for empty input", () => {
    expect(extractFileName("")).toBe("");
  });
});
