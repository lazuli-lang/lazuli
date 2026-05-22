Cell C.2 status
================

Commit hash: e4be3bb2b5ed43946e0e7697df19fa9c196915c2

Note: this is the current HEAD/base commit for the uncommitted implementation in this worktree.

Emitter file: `crates/lazuli_codegen_ts/src/cap_file_hooks.rs`

Golden test name: `cap_file_hook_emits_upload_orchestration_for_profile_photo`

Verification:

- `cargo test -p lazuli_codegen_ts`
- `cargo test -p lazuli_cli`
- `npx -p typescript tsc -p target/cap-file-hook-smoke --noEmit`

Sample emitted hook from `crates/lazuli_codegen_ts/tests/golden/cap-file-hooks/host.react.gen.ts`:

```ts
export function useHostProfilePhotoUpload(opts: HostProfilePhotoUploadOptions = {}): HostProfilePhotoUploadResult {
  const queryClient = useQueryClient();
  const [putInFlight, setPutInFlight] = useState(false);
  const [localError, setLocalError] = useState<unknown>(null);
  const invalidates = opts.invalidates ?? HOST_PROFILE_PHOTO_UPLOAD_DEFAULT_INVALIDATES;
  const onSuccess = opts.onSuccess;
  const onError = opts.onError;

  const requestUpload = useLazuliCommand(requestHostProfilePhotoUpload);
  const confirmUpload = useLazuliCommand(confirmHostProfilePhotoUpload);
  const clearUpload = useLazuliCommand(clearHostProfilePhoto);
  const displayUrl = useLazuliQuery(getHostProfilePhotoURL, {});

  const invalidateUploadTargets = useCallback(async () => {
    await Promise.all([
      ...invalidates.map((name) => queryClient.invalidateQueries({ queryKey: ["lazuli", name] })),
      queryClient.invalidateQueries({ queryKey: ["lazuli", getHostProfilePhotoURL.name] }),
    ]);
  }, [invalidates, queryClient]);

  const uploadFile = useCallback(async (file: File) => {
    const contentType = file.type || "application/octet-stream";
    try {
      setLocalError(null);
      if (file.size > 5242880) {
        throw lazuliFileSizeExceeded(5242880, file.size);
      }
      if (!lazuliFileMimeAccepted(HOST_PROFILE_PHOTO_UPLOAD_ACCEPT, contentType)) {
        throw lazuliFileMimeRejected(HOST_PROFILE_PHOTO_UPLOAD_ACCEPT, contentType);
      }
```
