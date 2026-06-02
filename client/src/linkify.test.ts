import { describe, expect, test } from "vitest";
import { linkifyText } from "./linkify";

describe("linkifyText", () => {
  test("returns a single text token for input with no URLs", () => {
    expect(linkifyText("no links here, just words")).toEqual([
      { type: "text", value: "no links here, just words" },
    ]);
  });

  test("returns an empty array for empty input", () => {
    expect(linkifyText("")).toEqual([]);
  });

  test("turns a bare https URL into a link token", () => {
    expect(linkifyText("https://example.com")).toEqual([
      { type: "link", url: "https://example.com" },
    ]);
  });

  test("recognizes http URLs too", () => {
    expect(linkifyText("see http://example.com/page for details")).toEqual([
      { type: "text", value: "see " },
      { type: "link", url: "http://example.com/page" },
      { type: "text", value: " for details" },
    ]);
  });

  test("preserves text around a URL", () => {
    expect(linkifyText("before https://example.com after")).toEqual([
      { type: "text", value: "before " },
      { type: "link", url: "https://example.com" },
      { type: "text", value: " after" },
    ]);
  });

  test("strips trailing prose punctuation from the link", () => {
    expect(linkifyText("see https://example.com, ok?")).toEqual([
      { type: "text", value: "see " },
      { type: "link", url: "https://example.com" },
      { type: "text", value: ", ok?" },
    ]);
  });

  test("handles a URL at end of sentence", () => {
    expect(linkifyText("go to https://example.com.")).toEqual([
      { type: "text", value: "go to " },
      { type: "link", url: "https://example.com" },
      { type: "text", value: "." },
    ]);
  });

  test("supports multiple URLs in one string", () => {
    expect(linkifyText("a https://one.test and b https://two.test c")).toEqual([
      { type: "text", value: "a " },
      { type: "link", url: "https://one.test" },
      { type: "text", value: " and b " },
      { type: "link", url: "https://two.test" },
      { type: "text", value: " c" },
    ]);
  });

  test("treats newlines as URL separators like ordinary whitespace", () => {
    expect(
      linkifyText("before https://one.test\nhttps://two.test/path after\nsee https://three.test."),
    ).toEqual([
      { type: "text", value: "before " },
      { type: "link", url: "https://one.test" },
      { type: "text", value: "\n" },
      { type: "link", url: "https://two.test/path" },
      { type: "text", value: " after\nsee " },
      { type: "link", url: "https://three.test" },
      { type: "text", value: "." },
    ]);
  });

  test("URL with path and query params", () => {
    expect(linkifyText("check https://example.com/path?x=1&y=2")).toEqual([
      { type: "text", value: "check " },
      { type: "link", url: "https://example.com/path?x=1&y=2" },
    ]);
  });

  test("URL adjacent to a closing paren is cleaned", () => {
    // The opening paren is not part of the match (regex starts at `h`).
    expect(linkifyText("(https://example.com)")).toEqual([
      { type: "text", value: "(" },
      { type: "link", url: "https://example.com" },
      { type: "text", value: ")" },
    ]);
  });

  test("leaves javascript: URLs alone", () => {
    // The regex only matches http(s), but even if a variant slipped in the
    // URL-parser check rejects non-http(s) schemes.
    expect(linkifyText("hey javascript:alert(1) haha")).toEqual([
      { type: "text", value: "hey javascript:alert(1) haha" },
    ]);
  });

  test("bare mention of a domain without scheme is not linkified", () => {
    // Consistent with the server's extractor — http(s) only.
    expect(linkifyText("visit example.com sometime")).toEqual([
      { type: "text", value: "visit example.com sometime" },
    ]);
  });
});
