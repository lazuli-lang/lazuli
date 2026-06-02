// Wire-boundary key transcoding between idiomatic JS (camelCase) and
// the Lazuli Go runtime's JSON contract (snake_case).
//
// Generated SDK interfaces present every field in camelCase so JS/TS
// callers write `{ chatId, sentAt }` instead of `{ chat_id, sent_at }`.
// The HTTP wire still uses snake_case because that's what the Go
// runtime emits/expects. `LazuliClient` runs every outbound body
// through `camelToSnakeDeep` before JSON.stringify and every inbound
// payload through `snakeToCamelDeep` after JSON.parse, so the entire
// re-keying happens at exactly one boundary point.
//
// Scope: only plain-object keys are remapped. Strings, numbers,
// booleans, null, and array elements pass through untouched. Class
// instances (non-plain objects) are also pass-through — Lazuli SDK
// outputs are always plain JSON shapes so this never matters in
// practice, but it keeps the helper safe against accidental Date /
// Map / RegExp inputs.

function camelToSnakeKey(key: string): string {
  // Already snake-cased keys (`org_id`) round-trip cleanly because
  // there's no uppercase to lift. ASCII-only — i18n field names
  // would need a richer transform; not a concern for Lazuli IR.
  let out = "";
  for (let i = 0; i < key.length; i++) {
    const ch = key.charCodeAt(i);
    if (ch >= 0x41 && ch <= 0x5a) {
      // Uppercase letter — prefix with `_` unless it's the first
      // char (rare in JS-land but possible: `OrgId` shouldn't
      // become `_org_id`).
      if (i > 0) {
        out += "_";
      }
      out += String.fromCharCode(ch + 32);
    } else {
      out += key[i];
    }
  }
  return out;
}

function snakeToCamelKey(key: string): string {
  // ASCII-only. Underscores at the very start or end are preserved
  // verbatim (`_secret` stays `_secret`) so we don't accidentally
  // collide with conventions outside the Lazuli IR.
  let out = "";
  let upper = false;
  for (let i = 0; i < key.length; i++) {
    const ch = key[i];
    if (ch === "_" && i > 0 && i < key.length - 1) {
      upper = true;
      continue;
    }
    if (upper) {
      out += ch!.toUpperCase();
      upper = false;
    } else {
      out += ch;
    }
  }
  return out;
}

function isPlainObject(value: unknown): value is Record<string, unknown> {
  if (value === null || typeof value !== "object") return false;
  const proto = Object.getPrototypeOf(value);
  return proto === Object.prototype || proto === null;
}

function deepRekey(
  value: unknown,
  mapKey: (k: string) => string,
): unknown {
  if (Array.isArray(value)) {
    return value.map((v) => deepRekey(v, mapKey));
  }
  if (isPlainObject(value)) {
    const out: Record<string, unknown> = {};
    for (const [k, v] of Object.entries(value)) {
      out[mapKey(k)] = deepRekey(v, mapKey);
    }
    return out;
  }
  return value;
}

/** Recursively convert all plain-object keys camelCase → snake_case. */
export function camelToSnakeDeep(value: unknown): unknown {
  return deepRekey(value, camelToSnakeKey);
}

/** Recursively convert all plain-object keys snake_case → camelCase. */
export function snakeToCamelDeep(value: unknown): unknown {
  return deepRekey(value, snakeToCamelKey);
}

/**
 * Outbound body re-keyer. The wire key for each field MUST equal the Go
 * `json:"…"` tag, which the emitter writes as the VERBATIM DSL field name.
 * For snake_case DSL fields (`tenant_id`, exposed to JS as `tenantId`)
 * the default heuristic `camelToSnakeKey` recovers the tag exactly. But
 * for an already-camelCase DSL field (`registrationStep`, exposed and
 * tagged identically) the heuristic would emit `registration_step` — a
 * key the Go decoder never matches — so the value is silently dropped on
 * the way OUT (the write-side mirror of the W1-4 read-side casing bug).
 *
 * `fields` pins the correct wire key for exactly those round-trip-unsafe
 * fields (`{ registrationStep: "registrationStep" }`). Generated SDK specs
 * carry it only when at least one field needs it; everything else falls
 * back to `camelToSnakeKey`, so snake_case payloads are unchanged.
 *
 * The override is applied at every nesting depth: a mapped key inside a
 * nested object or array element is rewritten the same way, since the Go
 * decoder matches tags structurally at each level.
 */
export function camelToWireDeep(
  value: unknown,
  fields?: Readonly<Record<string, string>>,
): unknown {
  if (fields === undefined) {
    return deepRekey(value, camelToSnakeKey);
  }
  const mapKey = (k: string): string =>
    Object.prototype.hasOwnProperty.call(fields, k) ? fields[k]! : camelToSnakeKey(k);
  return deepRekey(value, mapKey);
}
