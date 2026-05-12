package storage

import (
	"context"
	"errors"
	"io"
	"net/url"
	"time"

	"github.com/minio/minio-go/v7"
	"github.com/minio/minio-go/v7/pkg/credentials"
)

var _ ObjectStore = (*MinIOStore)(nil)

// MinIOStore wires ObjectStore to minio-go for MinIO and other
// S3-compatible providers such as R2, Backblaze, and Spaces.
type MinIOStore struct {
	Client *minio.Client
	Bucket string
}

func NewMinIOStore(endpoint, accessKey, secretKey, bucket string, useSSL bool) (*MinIOStore, error) {
	client, err := minio.New(endpoint, &minio.Options{
		Creds:  credentials.NewStaticV4(accessKey, secretKey, ""),
		Secure: useSSL,
	})
	if err != nil {
		return nil, err
	}
	return &MinIOStore{Client: client, Bucket: bucket}, nil
}

func (s *MinIOStore) Put(ctx context.Context, key Key, body io.Reader, contentType string) error {
	if s.Client == nil {
		return errors.New("lazuli/storage: MinIOStore has no client (call NewMinIOStore)")
	}
	_, err := s.Client.PutObject(ctx, s.Bucket, string(key), body, minioObjectSize(body), minio.PutObjectOptions{
		ContentType: contentType,
	})
	return err
}

func (s *MinIOStore) Get(ctx context.Context, key Key) (io.ReadCloser, error) {
	if s.Client == nil {
		return nil, errors.New("lazuli/storage: MinIOStore has no client (call NewMinIOStore)")
	}
	obj, err := s.Client.GetObject(ctx, s.Bucket, string(key), minio.GetObjectOptions{})
	if err != nil {
		if minioIsNotFound(err) {
			return nil, ErrFileNotFound
		}
		return nil, err
	}
	if _, err := obj.Stat(); err != nil {
		obj.Close()
		if minioIsNotFound(err) {
			return nil, ErrFileNotFound
		}
		return nil, err
	}
	return obj, nil
}

func (s *MinIOStore) Delete(ctx context.Context, key Key) error {
	if s.Client == nil {
		return errors.New("lazuli/storage: MinIOStore has no client (call NewMinIOStore)")
	}
	if _, err := s.Client.StatObject(ctx, s.Bucket, string(key), minio.StatObjectOptions{}); err != nil {
		if minioIsNotFound(err) {
			return ErrFileNotFound
		}
		return err
	}
	err := s.Client.RemoveObject(ctx, s.Bucket, string(key), minio.RemoveObjectOptions{})
	if minioIsNotFound(err) {
		return ErrFileNotFound
	}
	return err
}

func (s *MinIOStore) Sign(ctx context.Context, key Key, ttl time.Duration) (string, error) {
	if ttl <= 0 {
		return "", ErrVisibilityMismatch
	}
	if s.Client == nil {
		return "", errors.New("lazuli/storage: MinIOStore has no client (call NewMinIOStore)")
	}
	signed, err := s.Client.PresignedGetObject(ctx, s.Bucket, string(key), ttl, url.Values{})
	if err != nil {
		return "", err
	}
	return signed.String(), nil
}

type minioLenReader interface {
	Len() int
}

func minioObjectSize(body io.Reader) int64 {
	if body == nil {
		return -1
	}
	if sized, ok := body.(minioLenReader); ok {
		return int64(sized.Len())
	}
	return -1
}

func minioIsNotFound(err error) bool {
	var resp minio.ErrorResponse
	if !errors.As(err, &resp) {
		return false
	}
	switch resp.Code {
	case "NoSuchKey", "NoSuchObject", "NotFound":
		return true
	default:
		return false
	}
}
