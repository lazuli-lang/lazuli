package storage

import (
	"context"
	"errors"
	"io"
	"strings"
	"testing"

	"github.com/aws/aws-sdk-go-v2/aws"
	"github.com/aws/aws-sdk-go-v2/service/s3"
	s3types "github.com/aws/aws-sdk-go-v2/service/s3/types"
)

// fakeS3 is a tiny in-memory s3API implementation for tests. It
// keeps the wire-test surface narrow: PutObject stores bytes,
// GetObject returns them, DeleteObject drops them, NoSuchKey on
// miss. Production tests against live S3 sit in a separate
// `// +build integration` file the adapter team owns.
type fakeS3 struct {
	objects   map[string][]byte
	mimeTypes map[string]string
}

func newFakeS3() *fakeS3 {
	return &fakeS3{
		objects:   make(map[string][]byte),
		mimeTypes: make(map[string]string),
	}
}

func (f *fakeS3) PutObject(_ context.Context, in *s3.PutObjectInput, _ ...func(*s3.Options)) (*s3.PutObjectOutput, error) {
	body, err := io.ReadAll(in.Body)
	if err != nil {
		return nil, err
	}
	key := aws.ToString(in.Bucket) + "/" + aws.ToString(in.Key)
	f.objects[key] = body
	if in.ContentType != nil {
		f.mimeTypes[key] = *in.ContentType
	}
	return &s3.PutObjectOutput{}, nil
}

func (f *fakeS3) GetObject(_ context.Context, in *s3.GetObjectInput, _ ...func(*s3.Options)) (*s3.GetObjectOutput, error) {
	key := aws.ToString(in.Bucket) + "/" + aws.ToString(in.Key)
	body, ok := f.objects[key]
	if !ok {
		return nil, &s3types.NoSuchKey{}
	}
	return &s3.GetObjectOutput{
		Body:        io.NopCloser(strings.NewReader(string(body))),
		ContentType: aws.String(f.mimeTypes[key]),
	}, nil
}

func (f *fakeS3) DeleteObject(_ context.Context, in *s3.DeleteObjectInput, _ ...func(*s3.Options)) (*s3.DeleteObjectOutput, error) {
	key := aws.ToString(in.Bucket) + "/" + aws.ToString(in.Key)
	if _, ok := f.objects[key]; !ok {
		return nil, &s3types.NoSuchKey{}
	}
	delete(f.objects, key)
	return &s3.DeleteObjectOutput{}, nil
}

// TestS3StorePutGetDeleteRoundTrip wires the SDK calls through a
// fake `s3API`. Production hits live S3; this test only proves the
// wire is honest (no TODO panic) and that the typed sentinel
// `ErrFileNotFound` is surfaced from the NoSuchKey API error.
func TestS3StorePutGetDeleteRoundTrip(t *testing.T) {
	t.Parallel()
	fake := newFakeS3()
	store := &S3Store{Bucket: "test-bucket", Region: "us-east-1", client: fake}

	ctx := context.Background()
	payload := "hello,s3\n"
	if err := store.Put(ctx, Key("import/file"), strings.NewReader(payload), "text/csv"); err != nil {
		t.Fatalf("Put: %v", err)
	}

	rc, err := store.Get(ctx, Key("import/file"))
	if err != nil {
		t.Fatalf("Get: %v", err)
	}
	got, err := io.ReadAll(rc)
	if err != nil {
		t.Fatalf("ReadAll: %v", err)
	}
	rc.Close()
	if string(got) != payload {
		t.Fatalf("Get bytes = %q, want %q", got, payload)
	}

	if err := store.Delete(ctx, Key("import/file")); err != nil {
		t.Fatalf("Delete: %v", err)
	}
	if _, err := store.Get(ctx, Key("import/file")); !errors.Is(err, ErrFileNotFound) {
		t.Fatalf("Get after Delete: want ErrFileNotFound, got %v", err)
	}
}

// TestS3StoreGetMapsNoSuchKey pins that an SDK `NoSuchKey` error
// surfaces as the typed `ErrFileNotFound` sentinel so generated
// code can branch on it.
func TestS3StoreGetMapsNoSuchKey(t *testing.T) {
	t.Parallel()
	fake := newFakeS3()
	store := &S3Store{Bucket: "b", client: fake}
	if _, err := store.Get(context.Background(), Key("missing")); !errors.Is(err, ErrFileNotFound) {
		t.Fatalf("Get missing: want ErrFileNotFound, got %v", err)
	}
}

// TestS3StorePrefixApplied confirms the bucket Prefix is prepended
// to keys passed to the SDK.
func TestS3StorePrefixApplied(t *testing.T) {
	t.Parallel()
	fake := newFakeS3()
	store := &S3Store{Bucket: "b", Prefix: "tenant-42", client: fake}
	if err := store.Put(context.Background(), Key("avatar.png"), strings.NewReader("png"), "image/png"); err != nil {
		t.Fatalf("Put: %v", err)
	}
	if _, ok := fake.objects["b/tenant-42/avatar.png"]; !ok {
		t.Fatalf("expected prefix to be applied; objects = %v", fake.objects)
	}
}
