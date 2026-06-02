import { describe, expect, it } from "vitest";

import { camelToSnakeDeep, camelToWireDeep, snakeToCamelDeep } from "./case-mapper.js";

// Port of the SDK key caser `lower_camel_export` / runtime `snakeToCamelKey`:
// collapse interior `_`/`-` separators by upper-casing the next char, leave
// everything else (already-camel/Pascal/acronym names) untouched. This is the
// function the codegen uses to turn a verbatim Go json tag into the camelCase
// SDK key the caller holds.
function sdkKey(tag: string): string {
  let out = "";
  let upper = false;
  for (let i = 0; i < tag.length; i++) {
    const ch = tag[i]!;
    if ((ch === "_" || ch === "-") && i > 0 && i < tag.length - 1) {
      upper = true;
      continue;
    }
    if (upper) {
      out += ch.toUpperCase();
      upper = false;
    } else {
      out += ch;
    }
  }
  return out;
}

// Mirror of the codegen `wire_key_fields_literal`: pin a wire key for any tag
// whose SDK key would NOT round-trip back to the verbatim tag through the
// default outbound caser. (camelToSnakeDeep on a single-key object is the
// observable default transform.)
function wireKeyMap(tags: string[]): Record<string, string> | undefined {
  const map: Record<string, string> = {};
  for (const tag of tags) {
    const key = sdkKey(tag);
    const roundTripped = camelToSnakeDeep({ [key]: 0 }) as Record<string, unknown>;
    if (!Object.prototype.hasOwnProperty.call(roundTripped, tag)) {
      map[key] = tag;
    }
  }
  return Object.keys(map).length > 0 ? map : undefined;
}

describe("camelToWireDeep — outbound wire-key invariant", () => {
  // THE write-side mirror of the W1-4 read-side invariant. For every field
  // shape, the wire key the client emits MUST equal the Go json tag, which the
  // emitter writes as the VERBATIM DSL field name. Otherwise the request body
  // key mismatches what the Go decoder expects and the value is dropped on the
  // way OUT (TS-OUTBOUND-CAMEL-WRITE-LOSS).
  //
  //   outbound_wire_key(field) == go_json_tag(field) == field
  it("emits the verbatim Go json tag for every field shape", () => {
    // Each entry is a distinct field shape the DSL admits (== the Go json
    // tag). NB: `registration_step` and `registrationStep` both fold to the
    // SDK key `registrationStep`, so the DSL can never declare both in one
    // command — each tag is therefore exercised in its OWN one-field body,
    // which is exactly how the per-field wire-key map is keyed.
    const tags = [
      // snake_case
      "tenant_id",
      "org_id",
      "created_at",
      "registration_step",
      "provider_payment_id",
      // already camelCase (the live bug)
      "registrationStep",
      "apiKey",
      // PascalCase
      "OrgId",
      "HTMLBody",
      // bare acronym
      "URL",
      // single lowercase word
      "token",
      "role",
    ];

    for (const tag of tags) {
      const fields = wireKeyMap([tag]);
      // The caller object is keyed by the camelCase SDK key (what the
      // generated interface declares).
      const wire = camelToWireDeep({ [sdkKey(tag)]: `v:${tag}` }, fields) as Record<
        string,
        unknown
      >;
      expect(
        Object.prototype.hasOwnProperty.call(wire, tag),
        `wire body must carry the verbatim Go json tag \`${tag}\`; got keys ${JSON.stringify(
          Object.keys(wire),
        )}`,
      ).toBe(true);
      expect(wire[tag]).toBe(`v:${tag}`);
    }
  });

  it("camelCase DSL field stays camelCase on the wire (registrationStep regression)", () => {
    // Before: camelToSnakeDeep turned `registrationStep` -> `registration_step`,
    // which the Go decoder (json tag `registrationStep`) never matched -> dropped.
    const fields = wireKeyMap(["registrationStep"]);
    const wire = camelToWireDeep({ registrationStep: 7 }, fields) as Record<string, unknown>;
    expect(wire).toEqual({ registrationStep: 7 });
    // And the OLD behaviour would have produced the broken key:
    expect(camelToSnakeDeep({ registrationStep: 7 })).toEqual({ registration_step: 7 });
  });

  it("snake_case DSL field still goes out as snake_case (tenant_id)", () => {
    // tenant_id is exposed to JS as tenantId; it round-trips cleanly so no
    // override is emitted and the default caser handles it.
    expect(wireKeyMap(["tenant_id"])).toBeUndefined();
    const wire = camelToWireDeep({ tenantId: 1 }, undefined) as Record<string, unknown>;
    expect(wire).toEqual({ tenant_id: 1 });
  });

  it("applies overrides at nested depth and through arrays", () => {
    const fields = { registrationStep: "registrationStep" };
    const wire = camelToWireDeep(
      { items: [{ registrationStep: 1, tenantId: 2 }], registrationStep: 3 },
      fields,
    ) as Record<string, unknown>;
    expect(wire).toEqual({
      items: [{ registrationStep: 1, tenant_id: 2 }],
      registrationStep: 3,
    });
  });

  it("with no field map, behaves exactly like camelToSnakeDeep", () => {
    const body = { tenantId: 1, createdAt: "x", nested: { fullName: "a" } };
    expect(camelToWireDeep(body, undefined)).toEqual(camelToSnakeDeep(body));
  });

  it("inbound snakeToCamelDeep still folds verbatim tags to the SDK key", () => {
    // Read-side symmetry sanity: the inbound mapper turns each verbatim tag
    // back into the camelCase SDK key the interface declares.
    expect(snakeToCamelDeep({ tenant_id: 1, registrationStep: 2 })).toEqual({
      tenantId: 1,
      registrationStep: 2,
    });
  });
});
