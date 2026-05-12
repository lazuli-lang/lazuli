// Package jsonx contains JSON helpers for streaming responses and writers.
package jsonx

import (
	"bytes"
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"net/http"
)

const (
	contentTypeJSON    = "application/json"
	xAccelBufferingOff = "no"
)

var (
	// ErrNilWriter is returned when a stream writer is used without an io.Writer.
	ErrNilWriter = errors.New("lazuli/jsonx: writer is nil")

	// ErrStreamClosed is returned when writing to or closing an already closed stream.
	ErrStreamClosed = errors.New("lazuli/jsonx: stream is closed")

	// ErrStreamAborted is returned when writing to or closing an aborted stream.
	ErrStreamAborted = errors.New("lazuli/jsonx: stream is aborted")

	// ErrInvalidJSON is returned when a custom encoder does not produce one JSON value.
	ErrInvalidJSON = errors.New("lazuli/jsonx: invalid json")
)

// ArrayWriter streams a single JSON array to an io.Writer.
//
// ArrayWriter writes delimiters as values are appended, so callers can emit
// large arrays without buffering the full response. It is not safe for
// concurrent use. When the underlying writer is an http.ResponseWriter,
// ArrayWriter sets stream-friendly JSON response headers and flushes after each
// successful write.
type ArrayWriter struct {
	stream streamWriter
}

// NewArrayWriter returns a writer for one streamed JSON array.
func NewArrayWriter(w io.Writer) *ArrayWriter {
	return &ArrayWriter{
		stream: newStreamWriter(w, '[', ']', "array"),
	}
}

// Write appends value as the next JSON array element.
func (w *ArrayWriter) Write(value any) error {
	if err := w.stream.checkWritable(); err != nil {
		return err
	}

	payload, err := json.Marshal(value)
	if err != nil {
		return fmt.Errorf("lazuli/jsonx: encode array item: %w", err)
	}
	return w.stream.writeValue(payload)
}

// Close writes the closing array bracket.
func (w *ArrayWriter) Close() error {
	return w.stream.close()
}

// Abort marks the array stream as abandoned without writing a closing bracket.
func (w *ArrayWriter) Abort() {
	w.stream.abort()
}

// ObjectWriter streams a single JSON object to an io.Writer.
//
// ObjectWriter writes delimiters as fields are appended, so callers can emit
// large objects without buffering the full response. It is not safe for
// concurrent use. When the underlying writer is an http.ResponseWriter,
// ObjectWriter sets stream-friendly JSON response headers and flushes after
// each successful write.
type ObjectWriter struct {
	stream streamWriter
}

// NewObjectWriter returns a writer for one streamed JSON object.
func NewObjectWriter(w io.Writer) *ObjectWriter {
	return &ObjectWriter{
		stream: newStreamWriter(w, '{', '}', "object"),
	}
}

// Write appends name and value as the next JSON object field.
func (w *ObjectWriter) Write(name string, value any) error {
	if err := w.stream.checkWritable(); err != nil {
		return err
	}

	key, err := json.Marshal(name)
	if err != nil {
		return fmt.Errorf("lazuli/jsonx: encode object key: %w", err)
	}
	val, err := json.Marshal(value)
	if err != nil {
		return fmt.Errorf("lazuli/jsonx: encode object field %q: %w", name, err)
	}

	payload := make([]byte, 0, len(key)+1+len(val))
	payload = append(payload, key...)
	payload = append(payload, ':')
	payload = append(payload, val...)
	return w.stream.writeFragment(payload)
}

// Close writes the closing object brace.
func (w *ObjectWriter) Close() error {
	return w.stream.close()
}

// Abort marks the object stream as abandoned without writing a closing brace.
func (w *ObjectWriter) Abort() {
	w.stream.abort()
}

// StreamArray streams items as one JSON array.
//
// When encodeFn is nil, items are encoded with encoding/json. Otherwise
// encodeFn must write exactly one JSON value for each item. StreamArray checks
// ctx before each item and before closing the array; when the context is done,
// it aborts the stream and returns ctx.Err().
func StreamArray[T any](ctx context.Context, w io.Writer, items []T, encodeFn func(io.Writer, T) error) error {
	if ctx == nil {
		ctx = context.Background()
	}

	array := NewArrayWriter(w)
	for _, item := range items {
		if err := ctx.Err(); err != nil {
			array.Abort()
			return err
		}

		if encodeFn == nil {
			if err := array.Write(item); err != nil {
				array.Abort()
				return err
			}
			continue
		}

		if err := array.stream.checkWritable(); err != nil {
			array.Abort()
			return err
		}
		payload, err := encodeValue(func(w io.Writer) error {
			return encodeFn(w, item)
		})
		if err != nil {
			array.Abort()
			return fmt.Errorf("lazuli/jsonx: encode array item: %w", err)
		}
		if err := array.stream.writeValue(payload); err != nil {
			array.Abort()
			return err
		}
	}

	if err := ctx.Err(); err != nil {
		array.Abort()
		return err
	}
	return array.Close()
}

type streamWriter struct {
	w       io.Writer
	open    byte
	end     byte
	name    string
	started bool
	closed  bool
	aborted bool
	err     error
}

func newStreamWriter(w io.Writer, open, closeByte byte, name string) streamWriter {
	return streamWriter{
		w:    w,
		open: open,
		end:  closeByte,
		name: name,
	}
}

func (w *streamWriter) writeValue(payload []byte) error {
	return w.writePayload(payload, true)
}

func (w *streamWriter) writeFragment(payload []byte) error {
	return w.writePayload(payload, false)
}

func (w *streamWriter) writePayload(payload []byte, validate bool) error {
	if err := w.checkWritable(); err != nil {
		return err
	}
	if validate && !json.Valid(payload) {
		return fmt.Errorf("%w: %s item", ErrInvalidJSON, w.name)
	}

	prefix := []byte{','}
	if !w.started {
		prefix[0] = w.open
	}

	if err := w.write(prefix); err != nil {
		return err
	}
	if err := w.write(payload); err != nil {
		return err
	}

	w.started = true
	return w.flush()
}

func (w *streamWriter) close() error {
	if err := w.checkWritable(); err != nil {
		return err
	}

	if w.started {
		if err := w.write([]byte{w.end}); err != nil {
			return err
		}
	} else {
		if err := w.write([]byte{w.open, w.end}); err != nil {
			return err
		}
	}

	w.closed = true
	return w.flush()
}

func (w *streamWriter) abort() {
	if !w.closed {
		w.aborted = true
	}
}

func (w *streamWriter) checkWritable() error {
	switch {
	case w.err != nil:
		return w.err
	case w.w == nil:
		return ErrNilWriter
	case w.closed:
		return ErrStreamClosed
	case w.aborted:
		return ErrStreamAborted
	default:
		return nil
	}
}

func (w *streamWriter) write(p []byte) error {
	prepareResponse(w.w)

	n, err := w.w.Write(p)
	if err != nil {
		w.err = fmt.Errorf("lazuli/jsonx: write %s stream: %w", w.name, err)
		return w.err
	}
	if n != len(p) {
		w.err = fmt.Errorf("lazuli/jsonx: write %s stream: %w", w.name, io.ErrShortWrite)
		return w.err
	}
	return nil
}

func (w *streamWriter) flush() error {
	rw, ok := w.w.(http.ResponseWriter)
	if !ok {
		return nil
	}

	if err := http.NewResponseController(rw).Flush(); err != nil && !errors.Is(err, http.ErrNotSupported) {
		w.err = fmt.Errorf("lazuli/jsonx: flush %s stream: %w", w.name, err)
		return w.err
	}
	return nil
}

func prepareResponse(w io.Writer) {
	rw, ok := w.(http.ResponseWriter)
	if !ok {
		return
	}

	header := rw.Header()
	header.Set("Content-Type", contentTypeJSON)
	header.Set("X-Accel-Buffering", xAccelBufferingOff)
}

func encodeValue(encodeFn func(io.Writer) error) ([]byte, error) {
	var buf bytes.Buffer
	if err := encodeFn(&buf); err != nil {
		return nil, err
	}

	payload := bytes.TrimSpace(buf.Bytes())
	if !json.Valid(payload) {
		return nil, ErrInvalidJSON
	}
	return payload, nil
}
