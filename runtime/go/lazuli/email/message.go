package email

import (
	"errors"
	"fmt"
	"mime"
	netmail "net/mail"
	"strings"
	"unicode"
)

// Message is the provider-neutral shape for a transactional email.
// Bulk delivery is represented as a slice of Message values validated by
// ValidateBulk.
type Message struct {
	// From is the required sender mailbox.
	From Address
	// ReplyTo is an optional reply mailbox.
	ReplyTo *Address

	// To, Cc, and Bcc are recipient lists. At least one recipient is required.
	To  []Address
	Cc  []Address
	Bcc []Address

	// Subject is the required message subject.
	Subject string
	// TextBody is the optional plain-text body.
	TextBody string
	// HTMLBody is the optional HTML body. At least one body is required.
	HTMLBody string

	// Attachments are regular file attachments.
	Attachments []Attachment
	// InlineAttachments are file attachments addressable from the body via cid: URLs.
	InlineAttachments []InlineAttachment
}

// Address is a mailbox split into display name and addr-spec.
// Email must be the raw address only, for example "user@example.com".
type Address struct {
	// Name is the optional display name.
	Name string
	// Email is the required addr-spec, for example "user@example.com".
	Email string
}

// Attachment is a raw message attachment. ContentType is a concrete MIME
// media type such as "application/pdf"; Content holds the raw unencoded bytes.
type Attachment struct {
	// Name is the filename presented to the recipient.
	Name string
	// ContentType is a concrete MIME media type.
	ContentType string
	// Content holds raw, unencoded attachment bytes.
	Content []byte
}

// Size returns the raw, unencoded attachment byte count.
func (a Attachment) Size() int64 {
	return int64(len(a.Content))
}

// InlineAttachment is an attachment referenced from the message body via
// cid:<ContentID>. ContentID is stored without angle brackets or a cid: prefix.
type InlineAttachment struct {
	Attachment
	// ContentID is the cid reference token, without angle brackets or a cid: prefix.
	ContentID string
}

// Size returns the raw, unencoded inline attachment byte count.
func (a InlineAttachment) Size() int64 {
	return a.Attachment.Size()
}

// MessageLimits caps provider-neutral message dimensions. Zero means no limit.
type MessageLimits struct {
	// MaxRecipients caps To + Cc + Bcc recipients on one Message.
	MaxRecipients int
	// MaxAttachments caps regular plus inline attachments on one Message.
	MaxAttachments int
	// MaxAttachmentBytes caps each individual raw attachment body.
	MaxAttachmentBytes int64
	// MaxTotalAttachmentBytes caps all raw attachment bodies on one Message.
	MaxTotalAttachmentBytes int64
	// MaxMessages caps the number of messages accepted by ValidateBulk.
	MaxMessages int
}

var (
	// ErrInvalidMessage is wrapped by structural validation failures.
	ErrInvalidMessage = errors.New("email: invalid message")
	// ErrMessageSizeExceeded is wrapped when a message exceeds configured byte limits.
	ErrMessageSizeExceeded = errors.New("email: message size exceeded")
)

// ValidationError adds a field path to a message validation failure.
type ValidationError struct {
	Field string
	Err   error
}

// Error implements error.
func (e *ValidationError) Error() string {
	if e.Field == "" {
		return e.Err.Error()
	}
	return e.Field + ": " + e.Err.Error()
}

// Unwrap returns the underlying validation error.
func (e *ValidationError) Unwrap() error {
	return e.Err
}

// RecipientCount returns the number of To, Cc, and Bcc recipients.
func (m Message) RecipientCount() int {
	return len(m.To) + len(m.Cc) + len(m.Bcc)
}

// AttachmentCount returns the number of regular plus inline attachments.
func (m Message) AttachmentCount() int {
	return len(m.Attachments) + len(m.InlineAttachments)
}

// AttachmentBytes returns the raw, unencoded byte count for all attachments.
func (m Message) AttachmentBytes() int64 {
	var total int64
	for _, a := range m.Attachments {
		total += a.Size()
	}
	for _, a := range m.InlineAttachments {
		total += a.Size()
	}
	return total
}

// ValidateMessage validates a single transactional email payload.
func ValidateMessage(m Message, limits MessageLimits) error {
	if err := ValidateAddress(m.From); err != nil {
		return fieldError("from", err)
	}
	if m.ReplyTo != nil {
		if err := ValidateAddress(*m.ReplyTo); err != nil {
			return fieldError("reply_to", err)
		}
	}
	if err := validateAddressList("to", m.To); err != nil {
		return err
	}
	if err := validateAddressList("cc", m.Cc); err != nil {
		return err
	}
	if err := validateAddressList("bcc", m.Bcc); err != nil {
		return err
	}
	if m.RecipientCount() == 0 {
		return invalidf("message requires at least one recipient")
	}
	if limits.MaxRecipients > 0 && m.RecipientCount() > limits.MaxRecipients {
		return invalidf("recipient count %d exceeds limit %d", m.RecipientCount(), limits.MaxRecipients)
	}
	if strings.TrimSpace(m.Subject) == "" {
		return fieldError("subject", invalidf("subject is required"))
	}
	if hasLineBreakOrNUL(m.Subject) {
		return fieldError("subject", invalidf("subject contains a line break or NUL"))
	}
	if m.TextBody == "" && m.HTMLBody == "" {
		return invalidf("message requires text or HTML body")
	}
	if limits.MaxAttachments > 0 && m.AttachmentCount() > limits.MaxAttachments {
		return invalidf("attachment count %d exceeds limit %d", m.AttachmentCount(), limits.MaxAttachments)
	}

	var total int64
	for i, a := range m.Attachments {
		if err := ValidateAttachment(a, limits.MaxAttachmentBytes); err != nil {
			return fieldError(fmt.Sprintf("attachments[%d]", i), err)
		}
		total += a.Size()
	}

	contentIDs := make(map[string]struct{}, len(m.InlineAttachments))
	for i, a := range m.InlineAttachments {
		if err := ValidateInlineAttachment(a, limits.MaxAttachmentBytes); err != nil {
			return fieldError(fmt.Sprintf("inline_attachments[%d]", i), err)
		}
		if _, exists := contentIDs[a.ContentID]; exists {
			return fieldError(fmt.Sprintf("inline_attachments[%d].content_id", i), invalidf("duplicate content id %q", a.ContentID))
		}
		contentIDs[a.ContentID] = struct{}{}
		total += a.Size()
	}

	if limits.MaxTotalAttachmentBytes > 0 && total > limits.MaxTotalAttachmentBytes {
		return fieldError("attachments", fmt.Errorf("%w: total attachment bytes %d exceeds limit %d", ErrMessageSizeExceeded, total, limits.MaxTotalAttachmentBytes))
	}
	return nil
}

// ValidateBulk validates a batch of messages for bulk delivery.
func ValidateBulk(messages []Message, limits MessageLimits) error {
	if len(messages) == 0 {
		return invalidf("bulk payload requires at least one message")
	}
	if limits.MaxMessages > 0 && len(messages) > limits.MaxMessages {
		return invalidf("message count %d exceeds limit %d", len(messages), limits.MaxMessages)
	}
	for i, message := range messages {
		if err := ValidateMessage(message, limits); err != nil {
			return fieldError(fmt.Sprintf("messages[%d]", i), err)
		}
	}
	return nil
}

// ValidateAddress validates a mailbox address without provider-specific rules.
func ValidateAddress(a Address) error {
	if strings.TrimSpace(a.Email) == "" {
		return invalidf("address email is required")
	}
	if strings.TrimSpace(a.Email) != a.Email {
		return invalidf("address email has surrounding whitespace")
	}
	if containsControl(a.Email) {
		return invalidf("address email contains control characters")
	}
	if containsControl(a.Name) {
		return invalidf("address name contains control characters")
	}
	parsed, err := netmail.ParseAddress(a.Email)
	if err != nil {
		return invalidf("invalid address %q", a.Email)
	}
	if parsed.Address != a.Email || parsed.Name != "" {
		return invalidf("address email must be an addr-spec without display name")
	}
	return nil
}

// ValidateAttachment validates one regular attachment and its raw byte limit.
func ValidateAttachment(a Attachment, maxBytes int64) error {
	if err := ValidateAttachmentName(a.Name); err != nil {
		return fieldError("name", err)
	}
	if err := ValidateContentType(a.ContentType); err != nil {
		return fieldError("content_type", err)
	}
	if a.Content == nil {
		return fieldError("content", invalidf("attachment content is required"))
	}
	if maxBytes > 0 && a.Size() > maxBytes {
		return fieldError("content", fmt.Errorf("%w: attachment %q is %d bytes, limit %d", ErrMessageSizeExceeded, a.Name, a.Size(), maxBytes))
	}
	return nil
}

// ValidateInlineAttachment validates one inline attachment and content ID.
func ValidateInlineAttachment(a InlineAttachment, maxBytes int64) error {
	if err := ValidateAttachment(a.Attachment, maxBytes); err != nil {
		return err
	}
	if err := ValidateContentID(a.ContentID); err != nil {
		return fieldError("content_id", err)
	}
	return nil
}

// ValidateAttachmentName validates a filename safe to place in MIME headers.
func ValidateAttachmentName(name string) error {
	if strings.TrimSpace(name) == "" {
		return invalidf("attachment name is required")
	}
	if strings.TrimSpace(name) != name {
		return invalidf("attachment name has surrounding whitespace")
	}
	if containsControl(name) {
		return invalidf("attachment name contains control characters")
	}
	if strings.ContainsAny(name, `/\`) || name == "." || name == ".." {
		return invalidf("attachment name must be a file name, not a path")
	}
	return nil
}

// ValidateContentType validates a concrete MIME media type.
func ValidateContentType(contentType string) error {
	if strings.TrimSpace(contentType) == "" {
		return invalidf("content type is required")
	}
	if strings.TrimSpace(contentType) != contentType {
		return invalidf("content type has surrounding whitespace")
	}
	if containsControl(contentType) {
		return invalidf("content type contains control characters")
	}
	mediaType, _, err := mime.ParseMediaType(contentType)
	if err != nil {
		return invalidf("invalid content type %q", contentType)
	}
	parts := strings.Split(mediaType, "/")
	if len(parts) != 2 || parts[0] == "" || parts[1] == "" {
		return invalidf("content type must be type/subtype")
	}
	if parts[0] == "*" || parts[1] == "*" {
		return invalidf("content type must be concrete")
	}
	return nil
}

// ValidateContentID validates a cid reference token for inline attachments.
func ValidateContentID(contentID string) error {
	if strings.TrimSpace(contentID) == "" {
		return invalidf("content id is required")
	}
	if strings.TrimSpace(contentID) != contentID {
		return invalidf("content id has surrounding whitespace")
	}
	if strings.HasPrefix(strings.ToLower(contentID), "cid:") {
		return invalidf("content id must omit cid: prefix")
	}
	for _, r := range contentID {
		if unicode.IsControl(r) || unicode.IsSpace(r) || r == '<' || r == '>' {
			return invalidf("content id contains invalid characters")
		}
	}
	return nil
}

func validateAddressList(field string, addresses []Address) error {
	for i, address := range addresses {
		if err := ValidateAddress(address); err != nil {
			return fieldError(fmt.Sprintf("%s[%d]", field, i), err)
		}
	}
	return nil
}

func fieldError(field string, err error) error {
	if err == nil {
		return nil
	}
	return &ValidationError{Field: field, Err: err}
}

func invalidf(format string, args ...any) error {
	return fmt.Errorf("%w: "+format, append([]any{ErrInvalidMessage}, args...)...)
}

func containsControl(s string) bool {
	for _, r := range s {
		if unicode.IsControl(r) {
			return true
		}
	}
	return false
}

func hasLineBreakOrNUL(s string) bool {
	return strings.ContainsAny(s, "\r\n\x00")
}
