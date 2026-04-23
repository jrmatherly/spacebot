// Unit tests for the mockMsal base64url encoding contract with the
// daemon's MockValidator (src/auth/testing.rs). The daemon parses the
// token body after base64url-decoding, so any deviation from the
// url-safe alphabet (plus stripping `=`) makes every VITE_AUTH_MOCK=1
// CI run fail authentication with no obvious signal.

import { describe, it, expect } from "vitest";
import { base64UrlEncode } from "../mockMsal";

describe("base64UrlEncode", () => {
	it("returns empty string for empty input", () => {
		expect(base64UrlEncode(new Uint8Array(0))).toBe("");
	});

	it("produces a round-trippable encoding for JSON payloads", () => {
		const payload = JSON.stringify({ principal_type: "user", oid: "alice" });
		const bytes = new TextEncoder().encode(payload);
		const encoded = base64UrlEncode(bytes);
		// Decode via atob after reversing the url-safe transform.
		const padded = encoded.replace(/-/g, "+").replace(/_/g, "/");
		const missing = (4 - (padded.length % 4)) % 4;
		const binary = atob(padded + "=".repeat(missing));
		const decoded = new Uint8Array(
			Array.from(binary).map((c) => c.charCodeAt(0)),
		);
		const roundTripped = new TextDecoder().decode(decoded);
		expect(roundTripped).toBe(payload);
	});

	it("replaces + with -", () => {
		// Payload chosen so the standard base64 encoding contains +.
		const bytes = new Uint8Array([0xfb, 0xff, 0xbf]);
		const encoded = base64UrlEncode(bytes);
		expect(encoded).not.toContain("+");
		// Sanity: the standard encoding WOULD contain +, proving we
		// exercised the replace path.
		const standard = btoa(
			String.fromCharCode(...Array.from(bytes)),
		);
		expect(standard).toContain("+");
	});

	it("replaces / with _", () => {
		// Payload chosen so the standard base64 encoding contains /.
		const bytes = new Uint8Array([0xff, 0xff, 0xff]);
		const encoded = base64UrlEncode(bytes);
		expect(encoded).not.toContain("/");
		const standard = btoa(
			String.fromCharCode(...Array.from(bytes)),
		);
		expect(standard).toContain("/");
	});

	it("strips trailing = padding", () => {
		// 1-byte input produces "zz==" in standard base64 (2 pad chars).
		const bytes = new Uint8Array([0xcf]);
		const encoded = base64UrlEncode(bytes);
		expect(encoded).not.toContain("=");
		expect(encoded).toBe(
			btoa(String.fromCharCode(0xcf)).replace(/=+$/, ""),
		);
	});

	it("handles bytes ≥ 0x80 correctly (non-ASCII bytes from TextEncoder)", () => {
		// UTF-8 encoding of "é" is 0xc3 0xa9. Both bytes are ≥ 0x80;
		// String.fromCharCode must be used, not String.fromCodePoint.
		const bytes = new TextEncoder().encode("é");
		expect(Array.from(bytes)).toEqual([0xc3, 0xa9]);
		const encoded = base64UrlEncode(bytes);
		// Decode and confirm the round-trip preserves the high bytes.
		const padded = encoded.replace(/-/g, "+").replace(/_/g, "/");
		const missing = (4 - (padded.length % 4)) % 4;
		const binary = atob(padded + "=".repeat(missing));
		const decoded = new Uint8Array(
			Array.from(binary).map((c) => c.charCodeAt(0)),
		);
		expect(Array.from(decoded)).toEqual([0xc3, 0xa9]);
	});
});
