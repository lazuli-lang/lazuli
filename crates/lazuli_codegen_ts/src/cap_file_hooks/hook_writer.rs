//! Per-site React-hook writer for `@cap.File` upload flows. Pulled
//! out of `mod.rs` so the dispatcher and helpers stay readable.

use std::fmt::Write;

use lazuli_ir as ir;

use super::{
    command_ident, escape_js_string, file_uri_property_name, format_accept_array, pascal_case,
    screaming_snake, to_snake_case, CapFileHookSite,
};

pub(super) fn write_hook(out: &mut String, feature: &ir::Feature, site: &CapFileHookSite<'_>) {
    let feature_pascal = pascal_case(&feature.name);
    let field_pascal = pascal_case(&site.field.name);
    let hook_name = format!("use{feature_pascal}{field_pascal}Upload");
    let options_name = format!("{feature_pascal}{field_pascal}UploadOptions");
    let result_name = format!("{feature_pascal}{field_pascal}UploadResult");
    let accept_const = format!(
        "{}_{}_UPLOAD_ACCEPT",
        screaming_snake(&feature.name),
        screaming_snake(&site.field.name)
    );
    let invalidates_const = format!(
        "{}_{}_UPLOAD_DEFAULT_INVALIDATES",
        screaming_snake(&feature.name),
        screaming_snake(&site.field.name)
    );
    let max_bytes = site.capability.max_size.bytes;
    let accept = format_accept_array(&site.capability.accept);
    let default_invalidates = format!(
        "{}.lookup_my_{}",
        feature.name,
        to_snake_case(&site.resource.name)
    );
    let request_ident = command_ident(&feature.name, &site.commands.request);
    let confirm_ident = command_ident(&feature.name, &site.commands.confirm);
    let clear_ident = command_ident(&feature.name, &site.commands.clear);
    let get_url_ident = command_ident(&feature.name, &site.commands.get_url);
    let uri_prop = file_uri_property_name(&site.field.name);

    writeln!(out, "const {accept_const} = {accept} as const;").ok();
    writeln!(
        out,
        "const {invalidates_const} = [\"{}\"] as const;",
        escape_js_string(&default_invalidates)
    )
    .ok();
    writeln!(out).ok();

    writeln!(out, "export type {options_name} = {{").ok();
    writeln!(
        out,
        "  /** Override default invalidation; defaults to the field's lookup_my query. */"
    )
    .ok();
    writeln!(out, "  invalidates?: string[];").ok();
    writeln!(out, "  onSuccess?: () => void;").ok();
    writeln!(out, "  onError?: (err: unknown) => void;").ok();
    writeln!(out, "}};").ok();
    writeln!(out).ok();

    writeln!(out, "export type {result_name} = {{").ok();
    writeln!(out, "  {uri_prop}: string | null;").ok();
    writeln!(out, "  isUploading: boolean;").ok();
    writeln!(out, "  error: unknown;").ok();
    writeln!(out, "  uploadFile: (file: File) => Promise<void>;").ok();
    writeln!(out, "  removeFile: () => Promise<void>;").ok();
    writeln!(out, "}};").ok();
    writeln!(out).ok();

    writeln!(
        out,
        "export function {hook_name}(opts: {options_name} = {{}}): {result_name} {{"
    )
    .ok();
    writeln!(out, "  const queryClient = useQueryClient();").ok();
    writeln!(
        out,
        "  const [putInFlight, setPutInFlight] = useState(false);"
    )
    .ok();
    writeln!(
        out,
        "  const [localError, setLocalError] = useState<unknown>(null);"
    )
    .ok();
    writeln!(
        out,
        "  const invalidates = opts.invalidates ?? {invalidates_const};"
    )
    .ok();
    writeln!(out, "  const onSuccess = opts.onSuccess;").ok();
    writeln!(out, "  const onError = opts.onError;").ok();
    writeln!(out).ok();
    writeln!(
        out,
        "  const requestUpload = useLazuliCommand({request_ident});"
    )
    .ok();
    writeln!(
        out,
        "  const confirmUpload = useLazuliCommand({confirm_ident});"
    )
    .ok();
    writeln!(
        out,
        "  const clearUpload = useLazuliCommand({clear_ident});"
    )
    .ok();
    writeln!(
        out,
        "  const displayUrl = useLazuliQuery({get_url_ident}, {{}});"
    )
    .ok();
    writeln!(out).ok();

    writeln!(
        out,
        "  const invalidateUploadTargets = useCallback(async () => {{"
    )
    .ok();
    writeln!(out, "    await Promise.all([").ok();
    writeln!(
        out,
        "      ...invalidates.map((name) => queryClient.invalidateQueries({{ queryKey: [\"lazuli\", name] }})),"
    )
    .ok();
    writeln!(
        out,
        "      queryClient.invalidateQueries({{ queryKey: [\"lazuli\", {get_url_ident}.name] }}),"
    )
    .ok();
    writeln!(out, "    ]);").ok();
    writeln!(out, "  }}, [invalidates, queryClient]);").ok();
    writeln!(out).ok();

    writeln!(
        out,
        "  const uploadFile = useCallback(async (file: File) => {{"
    )
    .ok();
    writeln!(
        out,
        "    const contentType = file.type || \"application/octet-stream\";"
    )
    .ok();
    writeln!(out, "    try {{").ok();
    writeln!(out, "      setLocalError(null);").ok();
    writeln!(out, "      if (file.size > {max_bytes}) {{").ok();
    writeln!(
        out,
        "        throw lazuliFileSizeExceeded({max_bytes}, file.size);"
    )
    .ok();
    writeln!(out, "      }}").ok();
    writeln!(
        out,
        "      if (!lazuliFileMimeAccepted({accept_const}, contentType)) {{"
    )
    .ok();
    writeln!(
        out,
        "        throw lazuliFileMimeRejected({accept_const}, contentType);"
    )
    .ok();
    writeln!(out, "      }}").ok();
    writeln!(
        out,
        "      const upload = await requestUpload.mutateAsync({{ contentType, sizeBytes: file.size }});"
    )
    .ok();
    writeln!(out, "      const headers: Record<string, string> = {{}};").ok();
    writeln!(out, "      if (upload.headersContentType) {{").ok();
    writeln!(
        out,
        "        headers[\"Content-Type\"] = upload.headersContentType;"
    )
    .ok();
    writeln!(out, "      }}").ok();
    writeln!(out, "      setPutInFlight(true);").ok();
    writeln!(out, "      try {{").ok();
    writeln!(out, "        const put = await fetch(upload.url, {{").ok();
    writeln!(out, "          method: upload.method || \"PUT\",").ok();
    writeln!(out, "          headers,").ok();
    writeln!(out, "          body: file,").ok();
    writeln!(out, "        }});").ok();
    writeln!(out, "        if (!put.ok) {{").ok();
    writeln!(
        out,
        "          throw lazuliUploadPutFailed(put.status, put.statusText);"
    )
    .ok();
    writeln!(out, "        }}").ok();
    writeln!(out, "      }} finally {{").ok();
    writeln!(out, "        setPutInFlight(false);").ok();
    writeln!(out, "      }}").ok();
    writeln!(
        out,
        "      await confirmUpload.mutateAsync({{ key: upload.key }});"
    )
    .ok();
    writeln!(out, "      await invalidateUploadTargets();").ok();
    writeln!(out, "      await displayUrl.refetch();").ok();
    writeln!(out, "      onSuccess?.();").ok();
    writeln!(out, "    }} catch (err) {{").ok();
    writeln!(out, "      setLocalError(err);").ok();
    writeln!(out, "      onError?.(err);").ok();
    writeln!(out, "      throw err;").ok();
    writeln!(out, "    }}").ok();
    writeln!(
        out,
        "  }}, [confirmUpload, displayUrl, invalidateUploadTargets, onError, onSuccess, requestUpload]);"
    )
    .ok();
    writeln!(out).ok();

    writeln!(out, "  const removeFile = useCallback(async () => {{").ok();
    writeln!(out, "    try {{").ok();
    writeln!(out, "      setLocalError(null);").ok();
    writeln!(out, "      await clearUpload.mutateAsync({{}});").ok();
    writeln!(out, "      await invalidateUploadTargets();").ok();
    writeln!(out, "      await displayUrl.refetch();").ok();
    writeln!(out, "      onSuccess?.();").ok();
    writeln!(out, "    }} catch (err) {{").ok();
    writeln!(out, "      setLocalError(err);").ok();
    writeln!(out, "      onError?.(err);").ok();
    writeln!(out, "      throw err;").ok();
    writeln!(out, "    }}").ok();
    writeln!(
        out,
        "  }}, [clearUpload, displayUrl, invalidateUploadTargets, onError, onSuccess]);"
    )
    .ok();
    writeln!(out).ok();

    writeln!(
        out,
        "  const error = localError ?? requestUpload.error ?? confirmUpload.error ?? clearUpload.error ?? displayUrl.error ?? null;"
    )
    .ok();
    writeln!(
        out,
        "  const isUploading = requestUpload.isPending || confirmUpload.isPending || clearUpload.isPending || putInFlight;"
    )
    .ok();
    writeln!(out).ok();
    writeln!(out, "  return {{").ok();
    writeln!(out, "    {uri_prop}: displayUrl.data?.url ?? null,").ok();
    writeln!(out, "    isUploading,").ok();
    writeln!(out, "    error,").ok();
    writeln!(out, "    uploadFile,").ok();
    writeln!(out, "    removeFile,").ok();
    writeln!(out, "  }};").ok();
    writeln!(out, "}}").ok();
    writeln!(out).ok();
}
