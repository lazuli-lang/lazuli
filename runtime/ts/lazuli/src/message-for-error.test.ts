import { describe, expect, it } from "vitest";

import { LazuliError } from "./error.js";
import { messageForLazuliError } from "./message-for-error.js";

describe("messageForLazuliError", () => {
  it("uses a LazuliError messageKey and scalar data", () => {
    const err = new LazuliError(413, {
      code: "file_too_large",
      message: "File is too large",
      message_key: "file_size_exceeded",
      data: { maxMb: 10, filename: "avatar.png" },
    });

    expect(messageForLazuliError(err)).toEqual({
      key: "file_size_exceeded",
      fallback: "File is too large",
      params: { maxMb: 10, filename: "avatar.png" },
      code: "file_too_large",
    });
  });

  it("falls back to a LazuliError code without a messageKey", () => {
    const err = new LazuliError(400, {
      code: "validation_failed",
      message: "Validation failed",
    });

    expect(messageForLazuliError(err)).toEqual({
      key: "validation_failed",
      fallback: "Validation failed",
      params: {},
      code: "validation_failed",
    });
  });

  it("maps network-shaped errors to the network descriptor", () => {
    const err = new Error("Fetch failed while offline");

    expect(messageForLazuliError(err)).toEqual({
      key: "errors.network",
      fallback: "Fetch failed while offline",
      params: {},
      code: "network",
    });
  });

  it("maps random errors to the unknown descriptor", () => {
    expect(messageForLazuliError(new Error("Boom"))).toEqual({
      key: "errors.unknown",
      fallback: "An unknown error occurred",
      params: {},
      code: "unknown",
    });
  });

  it("maps plain primitives to the unknown descriptor", () => {
    expect(messageForLazuliError("Boom")).toEqual({
      key: "errors.unknown",
      fallback: "An unknown error occurred",
      params: {},
      code: "unknown",
    });
    expect(messageForLazuliError(500)).toEqual({
      key: "errors.unknown",
      fallback: "An unknown error occurred",
      params: {},
      code: "unknown",
    });
  });

  it("filters LazuliError params to strings and numbers", () => {
    const err = new LazuliError(500, {
      code: "upload_failed",
      message: "Upload failed",
      message_key: "upload_failed",
      data: {
        attempts: 2,
        filename: "avatar.png",
        retryable: true,
        nested: { maxMb: 10 },
        tags: ["profile"],
      },
    });

    expect(messageForLazuliError(err).params).toEqual({
      attempts: 2,
      filename: "avatar.png",
    });
  });
});
