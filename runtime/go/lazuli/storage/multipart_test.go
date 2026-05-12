package storage_test

import (
	"context"
	"errors"
	"testing"
	"time"

	"lazuli.dev/runtime/lazuli/storage"
)

var _ storage.MultipartStore = (*multipartStoreStub)(nil)

func TestMultipartStoreContractUsesProviderNeutralStructs(t *testing.T) {
	t.Parallel()

	store := &multipartStoreStub{}
	session, err := store.CreateMultipart(context.Background(), storage.CreateMultipartInput{
		Key:         storage.Key("imports/file.csv"),
		ContentType: "text/csv",
		Size:        11,
		PartSize:    10,
		PartCount:   2,
	})
	if err != nil {
		t.Fatalf("CreateMultipart() error = %v", err)
	}

	upload, err := store.SignPart(context.Background(), storage.SignPartInput{
		Session:    session,
		PartNumber: 1,
		Size:       10,
		TTL:        time.Minute,
	})
	if err != nil {
		t.Fatalf("SignPart() error = %v", err)
	}
	if upload.Method != "PUT" || upload.URL == "" {
		t.Fatalf("SignPart() = %#v, want signed PUT URL", upload)
	}

	ref, err := store.CompleteMultipart(context.Background(), storage.CompleteMultipartInput{
		Session: session,
		Parts: []storage.MultipartPart{
			{Number: 1, Size: 10, Token: "part-1"},
			{Number: 2, Size: 1, Token: "part-2"},
		},
	})
	if err != nil {
		t.Fatalf("CompleteMultipart() error = %v", err)
	}
	if ref.Key != session.Key || ref.ContentType != session.ContentType || ref.Size != session.Size {
		t.Fatalf("CompleteMultipart() = %#v, want session-backed file ref", ref)
	}

	if err := store.AbortMultipart(context.Background(), storage.AbortMultipartInput{Session: session}); err != nil {
		t.Fatalf("AbortMultipart() error = %v", err)
	}
}

func TestValidateMultipartContentTypeUsesFileContractAccept(t *testing.T) {
	t.Parallel()

	contract := storage.FileContract{
		Accept: []storage.MimeType{{Family: "text", Subtype: "csv"}},
	}
	if err := storage.ValidateMultipartContentType(contract, "text/csv;charset=utf-8"); err != nil {
		t.Fatalf("ValidateMultipartContentType() accepted content type error = %v", err)
	}

	err := storage.ValidateMultipartContentType(contract, "application/pdf")
	if !errors.Is(err, storage.ErrFileMimeRejected) {
		t.Fatalf("ValidateMultipartContentType() error = %v, want ErrFileMimeRejected", err)
	}
}

func TestValidateMultipartPartCount(t *testing.T) {
	t.Parallel()

	limits := storage.MultipartLimits{MaxPartCount: 2}
	if err := storage.ValidateMultipartPartCount(2, limits); err != nil {
		t.Fatalf("ValidateMultipartPartCount() valid count error = %v", err)
	}

	for _, partCount := range []int{0, 3} {
		err := storage.ValidateMultipartPartCount(partCount, limits)
		if !errors.Is(err, storage.ErrMultipartPartCountInvalid) {
			t.Fatalf("ValidateMultipartPartCount(%d) error = %v, want ErrMultipartPartCountInvalid", partCount, err)
		}
	}
}

func TestValidateMultipartPartOrder(t *testing.T) {
	t.Parallel()

	parts := []storage.MultipartPart{
		{Number: 1, Size: 10, Token: "a"},
		{Number: 3, Size: 1, Token: "b"},
	}
	if err := storage.ValidateMultipartPartOrder(parts); err != nil {
		t.Fatalf("ValidateMultipartPartOrder() valid sparse order error = %v", err)
	}

	err := storage.ValidateMultipartPartOrder([]storage.MultipartPart{
		{Number: 2, Size: 10, Token: "a"},
		{Number: 1, Size: 1, Token: "b"},
	})
	if !errors.Is(err, storage.ErrMultipartPartOrderInvalid) {
		t.Fatalf("ValidateMultipartPartOrder() error = %v, want ErrMultipartPartOrderInvalid", err)
	}
}

func TestValidateMultipartPartsChecksSizesAndTotal(t *testing.T) {
	t.Parallel()

	contract := storage.FileContract{MaxSize: 11}
	limits := storage.MultipartLimits{MinPartSize: 10, MaxPartCount: 3}
	parts := []storage.MultipartPart{
		{Number: 1, Size: 10, Token: "a"},
		{Number: 2, Size: 1, Token: "b"},
	}
	if err := storage.ValidateMultipartParts(contract, parts, limits); err != nil {
		t.Fatalf("ValidateMultipartParts() valid parts error = %v", err)
	}

	err := storage.ValidateMultipartParts(contract, []storage.MultipartPart{
		{Number: 1, Size: 9, Token: "a"},
		{Number: 2, Size: 2, Token: "b"},
	}, limits)
	if !errors.Is(err, storage.ErrMultipartPartSizeInvalid) {
		t.Fatalf("ValidateMultipartParts() small non-final error = %v, want ErrMultipartPartSizeInvalid", err)
	}

	err = storage.ValidateMultipartParts(contract, []storage.MultipartPart{
		{Number: 1, Size: 10, Token: "a"},
		{Number: 2, Size: 2, Token: "b"},
	}, limits)
	if !errors.Is(err, storage.ErrFileSizeExceeded) {
		t.Fatalf("ValidateMultipartParts() oversized total error = %v, want ErrFileSizeExceeded", err)
	}
}

type multipartStoreStub struct {
	session storage.MultipartSession
}

func (s *multipartStoreStub) CreateMultipart(_ context.Context, input storage.CreateMultipartInput) (storage.MultipartSession, error) {
	s.session = storage.MultipartSession{
		ID:          "opaque-session",
		Key:         input.Key,
		ContentType: input.ContentType,
		Size:        input.Size,
		PartSize:    input.PartSize,
		PartCount:   input.PartCount,
		ExpiresAt:   time.Now().Add(time.Hour),
	}
	return s.session, nil
}

func (s *multipartStoreStub) SignPart(_ context.Context, input storage.SignPartInput) (storage.MultipartPartUpload, error) {
	return storage.MultipartPartUpload{
		Method:    "PUT",
		URL:       "https://storage.example.test/" + string(input.Session.Key),
		Headers:   map[string]string{"Content-Type": input.Session.ContentType},
		ExpiresAt: time.Now().Add(input.TTL),
	}, nil
}

func (s *multipartStoreStub) CompleteMultipart(_ context.Context, input storage.CompleteMultipartInput) (storage.FileRef, error) {
	return storage.FileRef{
		Key:         input.Session.Key,
		ContentType: input.Session.ContentType,
		Size:        input.Session.Size,
	}, nil
}

func (s *multipartStoreStub) AbortMultipart(context.Context, storage.AbortMultipartInput) error {
	return nil
}
