// Package jsonx provides JSON helpers used by Lazuli runtimes.
package jsonx

import (
	"bufio"
	"bytes"
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"io"
)

var (
	// ErrLineTooLong is wrapped by DecodeError when an NDJSON record exceeds
	// the configured maximum line size.
	ErrLineTooLong = errors.New("jsonx: ndjson line too long")
)

var (
	errNilReader = errors.New("jsonx: ndjson reader is nil")
	errNilWriter = errors.New("jsonx: ndjson writer is nil")
)

// Encoder writes newline-delimited JSON records to an io.Writer.
type Encoder struct {
	w io.Writer
}

// NewEncoder returns an Encoder that writes one JSON value per line to w.
func NewEncoder(w io.Writer) *Encoder {
	return &Encoder{w: w}
}

// Encode writes v as one JSON record followed by a newline.
//
// The context is checked before and after the write, and by the writer wrapper
// used by encoding/json while bytes are written. A nil context is treated as
// context.Background().
func (e *Encoder) Encode(ctx context.Context, v any) error {
	if e == nil || e.w == nil {
		return errNilWriter
	}

	ctx = contextOrBackground(ctx)
	if err := ctx.Err(); err != nil {
		return err
	}

	encoder := json.NewEncoder(contextWriter{ctx: ctx, w: e.w})
	if err := encoder.Encode(v); err != nil {
		if ctxErr := ctx.Err(); ctxErr != nil {
			return ctxErr
		}
		return err
	}
	return ctx.Err()
}

// Decoder reads newline-delimited JSON records from an io.Reader.
type Decoder struct {
	r       *bufio.Reader
	options DecoderOptions
	line    int
}

// DecoderOptions configures NDJSON decoding.
type DecoderOptions struct {
	// MaxLineBytes caps one NDJSON record, excluding a trailing LF or CRLF line
	// ending. Zero means no limit.
	MaxLineBytes int

	// SkipEmptyLines ignores empty or whitespace-only lines while decoding.
	SkipEmptyLines bool
}

// DecoderOption configures a Decoder.
type DecoderOption func(*DecoderOptions)

// WithMaxLineBytes caps one NDJSON record. Values less than or equal to zero
// disable the limit.
func WithMaxLineBytes(max int) DecoderOption {
	return func(options *DecoderOptions) {
		if max <= 0 {
			options.MaxLineBytes = 0
			return
		}
		options.MaxLineBytes = max
	}
}

// WithSkipEmptyLines configures whether Decode ignores empty or whitespace-only
// lines.
func WithSkipEmptyLines(skip bool) DecoderOption {
	return func(options *DecoderOptions) {
		options.SkipEmptyLines = skip
	}
}

// NewDecoder returns a Decoder that reads one JSON value per line from r.
func NewDecoder(r io.Reader, options ...DecoderOption) *Decoder {
	opts := DecoderOptions{}
	for _, option := range options {
		if option != nil {
			option(&opts)
		}
	}
	if opts.MaxLineBytes < 0 {
		opts.MaxLineBytes = 0
	}
	if r == nil {
		return &Decoder{options: opts}
	}
	return &Decoder{r: bufio.NewReader(r), options: opts}
}

// Decode reads the next NDJSON record into v.
//
// Decode returns io.EOF after the stream is exhausted. Invalid JSON and line
// size failures are returned as *DecodeError values carrying the physical line
// number. A nil context is treated as context.Background().
func (d *Decoder) Decode(ctx context.Context, v any) error {
	if d == nil || d.r == nil {
		return errNilReader
	}

	ctx = contextOrBackground(ctx)
	for {
		line, lineNumber, err := d.readLine(ctx)
		if err != nil {
			return err
		}
		if d.options.SkipEmptyLines && len(bytes.TrimSpace(line)) == 0 {
			continue
		}
		if err := ctx.Err(); err != nil {
			return err
		}
		if err := json.Unmarshal(line, v); err != nil {
			return &DecodeError{Line: lineNumber, Err: err}
		}
		return ctx.Err()
	}
}

// Line returns the last physical line number read by Decode.
func (d *Decoder) Line() int {
	if d == nil {
		return 0
	}
	return d.line
}

// DecodeError describes an NDJSON record that could not be decoded.
type DecodeError struct {
	// Line is the 1-based physical line number in the NDJSON stream.
	Line int

	// Err is the underlying decoding failure.
	Err error
}

// Error returns a stable, human-readable decode failure message.
func (e *DecodeError) Error() string {
	if e == nil {
		return "<nil>"
	}
	if e.Line > 0 {
		return fmt.Sprintf("jsonx: decode line %d: %v", e.Line, e.Err)
	}
	return fmt.Sprintf("jsonx: decode: %v", e.Err)
}

// Unwrap returns the underlying decoding failure.
func (e *DecodeError) Unwrap() error {
	if e == nil {
		return nil
	}
	return e.Err
}

func (d *Decoder) readLine(ctx context.Context) ([]byte, int, error) {
	if err := ctx.Err(); err != nil {
		return nil, 0, err
	}

	lineNumber := d.line + 1
	var line []byte
	for {
		if err := ctx.Err(); err != nil {
			return nil, lineNumber, err
		}

		chunk, err := d.r.ReadSlice('\n')
		if len(chunk) > 0 {
			if len(line) == 0 {
				d.line = lineNumber
			}
			line = append(line, chunk...)
			if d.options.MaxLineBytes > 0 && lineContentLen(line) > d.options.MaxLineBytes {
				if err == nil || errors.Is(err, io.EOF) {
					return nil, lineNumber, &DecodeError{Line: lineNumber, Err: ErrLineTooLong}
				}
				d.discardLine(ctx)
				return nil, lineNumber, &DecodeError{Line: lineNumber, Err: ErrLineTooLong}
			}
		}

		switch {
		case err == nil:
			return trimLineEnding(line), lineNumber, nil
		case errors.Is(err, io.EOF):
			if len(line) == 0 {
				return nil, 0, io.EOF
			}
			return trimLineEnding(line), lineNumber, nil
		case errors.Is(err, bufio.ErrBufferFull):
			continue
		default:
			if len(line) == 0 {
				d.line = lineNumber
			}
			return nil, lineNumber, &DecodeError{Line: lineNumber, Err: err}
		}
	}
}

func (d *Decoder) discardLine(ctx context.Context) {
	for {
		if err := ctx.Err(); err != nil {
			return
		}
		_, err := d.r.ReadSlice('\n')
		switch {
		case err == nil || errors.Is(err, io.EOF):
			return
		case errors.Is(err, bufio.ErrBufferFull):
			continue
		default:
			return
		}
	}
}

func lineContentLen(line []byte) int {
	n := len(line)
	if n > 0 && line[n-1] == '\n' {
		n--
	}
	if n > 0 && line[n-1] == '\r' {
		n--
	}
	return n
}

func trimLineEnding(line []byte) []byte {
	line = bytes.TrimSuffix(line, []byte("\n"))
	line = bytes.TrimSuffix(line, []byte("\r"))
	return line
}

func contextOrBackground(ctx context.Context) context.Context {
	if ctx == nil {
		return context.Background()
	}
	return ctx
}

type contextWriter struct {
	ctx context.Context
	w   io.Writer
}

func (w contextWriter) Write(p []byte) (int, error) {
	if err := w.ctx.Err(); err != nil {
		return 0, err
	}
	n, err := w.w.Write(p)
	if err != nil {
		return n, err
	}
	if err := w.ctx.Err(); err != nil {
		return n, err
	}
	return n, nil
}
