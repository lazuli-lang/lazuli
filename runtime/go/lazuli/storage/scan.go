package storage

import (
	"context"
	"errors"
	"fmt"
	"io"
)

// Verdict is the provider-neutral outcome of a storage scan hook.
type Verdict int

const (
	// VerdictClean means the hook found no reason to reject the file.
	VerdictClean Verdict = iota
	// VerdictInfected means the hook detected malware or another known
	// malicious signature in the file.
	VerdictInfected
	// VerdictBlocked means the hook rejected the file by policy without
	// claiming a malware detection, for example an encrypted archive that
	// cannot be inspected safely.
	VerdictBlocked
	// VerdictUnavailable means the hook could not complete a scan.
	VerdictUnavailable
)

// String renders v as the stable lowercase token used in logs and tests.
func (v Verdict) String() string {
	switch v {
	case VerdictClean:
		return "clean"
	case VerdictInfected:
		return "infected"
	case VerdictBlocked:
		return "blocked"
	case VerdictUnavailable:
		return "unavailable"
	default:
		return "unknown"
	}
}

// ScanResult is the normalized result returned by a storage scan hook.
type ScanResult struct {
	// Verdict is the hook's decision for this file.
	Verdict Verdict

	// Scanner identifies the hook/scanner that produced the result. It is
	// optional and intended for audit logs, not provider branching.
	Scanner string

	// Reason carries a short provider-neutral explanation or signature label.
	// It should not include raw file contents.
	Reason string
}

// BodyOpener opens a fresh stream for a file being scanned. RunScanHooks calls
// it once per hook and closes each returned body after that hook returns.
type BodyOpener func(ctx context.Context) (io.ReadCloser, error)

// ScanHook is implemented by antivirus, DLP, or policy scanners that can
// inspect a file before the runtime persists or exposes it.
type ScanHook interface {
	// Scan inspects body and returns a provider-neutral verdict. The hook must
	// consume only the stream it is given; RunScanHooks opens a fresh stream for
	// every hook so implementations do not need to coordinate seek state.
	Scan(ctx context.Context, metadata Metadata, body io.Reader) (ScanResult, error)
}

var (
	// ErrFileInfected is returned when any scan hook detects malware or another
	// known malicious signature. Maps to a rejected upload/download.
	ErrFileInfected = errors.New("lazuli/storage: file_infected")

	// ErrFileBlocked is returned when any scan hook rejects a file by policy
	// without a malware detection.
	ErrFileBlocked = errors.New("lazuli/storage: file_blocked")

	// ErrScanUnavailable is returned when scan hooks are configured but the
	// runtime cannot obtain a completed clean verdict from all of them.
	ErrScanUnavailable = errors.New("lazuli/storage: scan_unavailable")
)

var (
	errNilScanHook       = errors.New("nil scan hook")
	errNilScanBodyOpener = errors.New("nil scan body opener")
	errNilScanBody       = errors.New("nil scan body")
	errUnknownVerdict    = errors.New("unknown scan verdict")
)

// RunScanHooks runs every hook against a fresh file body and aggregates the
// results deterministically. All hooks are invoked in slice order. Clean is
// returned only when every hook returns VerdictClean with no error.
//
// Non-clean verdicts are aggregated by severity, independent of hook order:
// infected wins over blocked, blocked wins over unavailable, and unavailable
// wins over clean. When multiple hooks return the same winning severity, the
// earliest hook in the input slice supplies the returned ScanResult.
//
// Hook errors, body-open errors, body-close errors after a clean verdict, nil
// hooks, nil bodies, and unknown verdicts are classified as
// VerdictUnavailable. Context cancellation and deadline errors are propagated
// unchanged.
func RunScanHooks(
	ctx context.Context,
	hooks []ScanHook,
	metadata Metadata,
	open BodyOpener,
) (ScanResult, error) {
	if err := ctx.Err(); err != nil {
		return ScanResult{Verdict: VerdictUnavailable}, err
	}
	if len(hooks) == 0 {
		return ScanResult{Verdict: VerdictClean}, nil
	}

	aggregate := ScanResult{Verdict: VerdictClean}
	var aggregateErr error

	for _, hook := range hooks {
		result, err := runScanHook(ctx, hook, metadata, open)
		if err != nil {
			if isContextError(err) {
				return ScanResult{Verdict: VerdictUnavailable}, err
			}
		}

		if scanVerdictRank(result.Verdict) > scanVerdictRank(aggregate.Verdict) {
			aggregate = result
			aggregateErr = err
		}

		if err := ctx.Err(); err != nil {
			return ScanResult{Verdict: VerdictUnavailable}, err
		}
	}

	if aggregate.Verdict == VerdictClean {
		return aggregate, nil
	}

	err := scanVerdictError(aggregate.Verdict)
	if aggregate.Verdict == VerdictUnavailable && aggregateErr != nil {
		err = fmt.Errorf("%w: %v", err, aggregateErr)
	}
	return aggregate, err
}

func runScanHook(ctx context.Context, hook ScanHook, metadata Metadata, open BodyOpener) (ScanResult, error) {
	if err := ctx.Err(); err != nil {
		return ScanResult{Verdict: VerdictUnavailable}, err
	}
	if hook == nil {
		return ScanResult{Verdict: VerdictUnavailable}, errNilScanHook
	}
	if open == nil {
		return ScanResult{Verdict: VerdictUnavailable}, errNilScanBodyOpener
	}

	body, err := open(ctx)
	if err != nil {
		return ScanResult{Verdict: VerdictUnavailable}, err
	}
	if body == nil {
		return ScanResult{Verdict: VerdictUnavailable}, errNilScanBody
	}

	result, scanErr := hook.Scan(ctx, metadata, body)
	closeErr := body.Close()
	if scanErr != nil {
		return ScanResult{Verdict: VerdictUnavailable, Scanner: result.Scanner, Reason: result.Reason}, scanErr
	}
	if !isKnownScanVerdict(result.Verdict) {
		result.Verdict = VerdictUnavailable
		return result, errUnknownVerdict
	}
	if result.Verdict == VerdictClean && closeErr != nil {
		return ScanResult{Verdict: VerdictUnavailable, Scanner: result.Scanner, Reason: result.Reason}, closeErr
	}
	return result, nil
}

func scanVerdictError(verdict Verdict) error {
	switch verdict {
	case VerdictInfected:
		return ErrFileInfected
	case VerdictBlocked:
		return ErrFileBlocked
	case VerdictUnavailable:
		return ErrScanUnavailable
	default:
		return nil
	}
}

func scanVerdictRank(verdict Verdict) int {
	switch verdict {
	case VerdictInfected:
		return 3
	case VerdictBlocked:
		return 2
	case VerdictUnavailable:
		return 1
	case VerdictClean:
		return 0
	default:
		return 1
	}
}

func isKnownScanVerdict(verdict Verdict) bool {
	switch verdict {
	case VerdictClean, VerdictInfected, VerdictBlocked, VerdictUnavailable:
		return true
	default:
		return false
	}
}

func isContextError(err error) bool {
	return errors.Is(err, context.Canceled) || errors.Is(err, context.DeadlineExceeded)
}
