package email

import (
	"errors"
	"strings"
	"testing"
)

func TestValidateMessageAcceptsAttachmentsAndAccountsBytes(t *testing.T) {
	t.Parallel()

	message := validMessage()
	if got := message.RecipientCount(); got != 1 {
		t.Fatalf("RecipientCount = %d, want 1", got)
	}
	if got := message.AttachmentCount(); got != 2 {
		t.Fatalf("AttachmentCount = %d, want 2", got)
	}
	if got := message.AttachmentBytes(); got != 5 {
		t.Fatalf("AttachmentBytes = %d, want 5", got)
	}

	limits := MessageLimits{
		MaxRecipients:           2,
		MaxAttachments:          2,
		MaxAttachmentBytes:      4,
		MaxTotalAttachmentBytes: 5,
	}
	if err := ValidateMessage(message, limits); err != nil {
		t.Fatalf("ValidateMessage: %v", err)
	}
}

func TestValidateMessageRejectsTotalAttachmentLimit(t *testing.T) {
	t.Parallel()

	err := ValidateMessage(validMessage(), MessageLimits{MaxTotalAttachmentBytes: 4})
	if !errors.Is(err, ErrMessageSizeExceeded) {
		t.Fatalf("ValidateMessage error = %v, want ErrMessageSizeExceeded", err)
	}
	if !strings.Contains(err.Error(), "attachments") {
		t.Fatalf("ValidateMessage error = %q, want field path", err)
	}
}

func TestValidateMessageRejectsInvalidInlineContentID(t *testing.T) {
	t.Parallel()

	message := validMessage()
	message.InlineAttachments[0].ContentID = "<logo@assets>"

	err := ValidateMessage(message, MessageLimits{})
	if !errors.Is(err, ErrInvalidMessage) {
		t.Fatalf("ValidateMessage error = %v, want ErrInvalidMessage", err)
	}
	if !strings.Contains(err.Error(), "inline_attachments[0]") || !strings.Contains(err.Error(), "content_id") {
		t.Fatalf("ValidateMessage error = %q, want inline content-id field path", err)
	}
}

func TestValidateMessageRejectsDuplicateInlineContentID(t *testing.T) {
	t.Parallel()

	message := validMessage()
	message.InlineAttachments = append(message.InlineAttachments, InlineAttachment{
		Attachment: Attachment{
			Name:        "chart.png",
			ContentType: "image/png",
			Content:     []byte{6},
		},
		ContentID: "logo@assets",
	})

	err := ValidateMessage(message, MessageLimits{})
	if !errors.Is(err, ErrInvalidMessage) {
		t.Fatalf("ValidateMessage error = %v, want ErrInvalidMessage", err)
	}
	if !strings.Contains(err.Error(), "duplicate content id") {
		t.Fatalf("ValidateMessage error = %q, want duplicate content id", err)
	}
}

func TestValidateAttachmentRejectsInvalidMetadata(t *testing.T) {
	t.Parallel()

	tests := []struct {
		name       string
		attachment Attachment
		maxBytes   int64
		wantErr    error
		wantText   string
	}{
		{
			name: "path name",
			attachment: Attachment{
				Name:        "../report.pdf",
				ContentType: "application/pdf",
				Content:     []byte{1},
			},
			wantErr:  ErrInvalidMessage,
			wantText: "name",
		},
		{
			name: "wildcard content type",
			attachment: Attachment{
				Name:        "report.pdf",
				ContentType: "application/*",
				Content:     []byte{1},
			},
			wantErr:  ErrInvalidMessage,
			wantText: "content_type",
		},
		{
			name: "nil content",
			attachment: Attachment{
				Name:        "report.pdf",
				ContentType: "application/pdf",
			},
			wantErr:  ErrInvalidMessage,
			wantText: "content",
		},
		{
			name: "byte limit",
			attachment: Attachment{
				Name:        "report.pdf",
				ContentType: "application/pdf",
				Content:     []byte{1, 2},
			},
			maxBytes: 1,
			wantErr:  ErrMessageSizeExceeded,
			wantText: "content",
		},
	}

	for _, tt := range tests {
		tt := tt
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()

			err := ValidateAttachment(tt.attachment, tt.maxBytes)
			if !errors.Is(err, tt.wantErr) {
				t.Fatalf("ValidateAttachment error = %v, want %v", err, tt.wantErr)
			}
			if !strings.Contains(err.Error(), tt.wantText) {
				t.Fatalf("ValidateAttachment error = %q, want %q", err, tt.wantText)
			}
		})
	}
}

func TestValidateContentIDRejectsCIDPrefix(t *testing.T) {
	t.Parallel()

	err := ValidateContentID("cid:logo")
	if !errors.Is(err, ErrInvalidMessage) {
		t.Fatalf("ValidateContentID error = %v, want ErrInvalidMessage", err)
	}
}

func TestValidateBulkValidatesBatchShape(t *testing.T) {
	t.Parallel()

	messages := []Message{validMessage(), validMessage()}
	if err := ValidateBulk(messages, MessageLimits{MaxMessages: 2}); err != nil {
		t.Fatalf("ValidateBulk: %v", err)
	}

	err := ValidateBulk(messages, MessageLimits{MaxMessages: 1})
	if !errors.Is(err, ErrInvalidMessage) {
		t.Fatalf("ValidateBulk error = %v, want ErrInvalidMessage", err)
	}
	if !strings.Contains(err.Error(), "message count") {
		t.Fatalf("ValidateBulk error = %q, want message count", err)
	}
}

func TestValidateBulkReportsMessageIndex(t *testing.T) {
	t.Parallel()

	messages := []Message{validMessage(), validMessage()}
	messages[1].To[0].Email = "not an address"

	err := ValidateBulk(messages, MessageLimits{})
	if !errors.Is(err, ErrInvalidMessage) {
		t.Fatalf("ValidateBulk error = %v, want ErrInvalidMessage", err)
	}
	if !strings.Contains(err.Error(), "messages[1]: to[0]") {
		t.Fatalf("ValidateBulk error = %q, want indexed field path", err)
	}
}

func validMessage() Message {
	return Message{
		From: Address{
			Name:  "Acme",
			Email: "noreply@example.com",
		},
		To: []Address{
			{Email: "user@example.com"},
		},
		Subject:  "Report ready",
		TextBody: "Your report is ready.",
		HTMLBody: "<p>Your report is ready.</p>",
		Attachments: []Attachment{
			{
				Name:        "report.pdf",
				ContentType: "application/pdf",
				Content:     []byte{1, 2, 3},
			},
		},
		InlineAttachments: []InlineAttachment{
			{
				Attachment: Attachment{
					Name:        "logo.png",
					ContentType: "image/png",
					Content:     []byte{4, 5},
				},
				ContentID: "logo@assets",
			},
		},
	}
}
