package mercadopago

import (
	"bytes"
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"net/http"
	"net/url"
	"strconv"
	"strings"
	"time"

	"lazuli.dev/runtime/lazuli/payments"
)

const (
	// ProviderName is the provider identifier used by normalized payment records.
	ProviderName = "mercadopago"
	// DefaultBaseURL is the Mercado Pago REST API host.
	DefaultBaseURL = "https://api.mercadopago.com"
	// HeaderIdempotencyKey is the Mercado Pago idempotency header.
	HeaderIdempotencyKey = "X-Idempotency-Key"
)

var (
	// ErrAccessTokenMissing means the client cannot authenticate provider calls.
	ErrAccessTokenMissing = errors.New("mercadopago: access token missing")
	// ErrBaseURLInvalid means the configured provider base URL is malformed.
	ErrBaseURLInvalid = errors.New("mercadopago: base url invalid")
	// ErrPaymentIDMissing means a payment operation does not identify a provider payment.
	ErrPaymentIDMissing = errors.New("mercadopago: payment id missing")

	errCurrencyMissing     = errors.New("mercadopago: currency missing")
	errAmountInvalid       = errors.New("mercadopago: amount must be positive")
	errRefundAmountInvalid = errors.New("mercadopago: refund amount cannot be negative")
	errQuantityInvalid     = errors.New("mercadopago: item quantity must be positive")
)

var _ payments.PaymentGateway = (*Client)(nil)

// Client is a minimal Mercado Pago adapter for the generic payments contract.
type Client struct {
	AccessToken string
	BaseURL     string
	HTTPClient  *http.Client
}

// ClientOption customizes a new Client.
type ClientOption func(*Client)

// NewClient builds a Mercado Pago client. BaseURL and HTTPClient are injectable
// so tests and generated runtimes can avoid live network calls.
func NewClient(accessToken string, options ...ClientOption) *Client {
	client := &Client{
		AccessToken: accessToken,
		BaseURL:     DefaultBaseURL,
	}
	for _, option := range options {
		option(client)
	}
	return client
}

// WithBaseURL overrides the Mercado Pago API base URL.
func WithBaseURL(baseURL string) ClientOption {
	return func(client *Client) {
		client.BaseURL = baseURL
	}
}

// WithHTTPClient overrides the HTTP client used for provider calls.
func WithHTTPClient(httpClient *http.Client) ClientOption {
	return func(client *Client) {
		client.HTTPClient = httpClient
	}
}

// UnsupportedOperationError marks a generic payment operation that this
// Mercado Pago skeleton intentionally does not implement yet.
type UnsupportedOperationError struct {
	Operation string
}

func (e UnsupportedOperationError) Error() string {
	if e.Operation == "" {
		return "mercadopago: unsupported payment operation"
	}
	return "mercadopago: unsupported payment operation: " + e.Operation
}

func (e UnsupportedOperationError) Unwrap() error {
	return payments.ErrGatewayUnsupported
}

// APIError carries normalized provider error details.
type APIError struct {
	Operation  string
	StatusCode int
	Body       string
	Err        error
}

func (e *APIError) Error() string {
	if e == nil {
		return "<nil>"
	}
	operation := e.Operation
	if operation == "" {
		operation = "request"
	}
	if e.Body == "" {
		return fmt.Sprintf("mercadopago: %s failed: status %d", operation, e.StatusCode)
	}
	return fmt.Sprintf("mercadopago: %s failed: status %d: %s", operation, e.StatusCode, e.Body)
}

func (e *APIError) Unwrap() error {
	if e == nil {
		return nil
	}
	return e.Err
}

// CreatePaymentIntent creates a Checkout Pro preference and maps it to the
// provider-neutral payment intent shape.
func (c *Client) CreatePaymentIntent(
	ctx context.Context,
	req payments.CreatePaymentIntentRequest,
) (payments.PaymentIntent, error) {
	payload, err := newPreferencePayload(req)
	if err != nil {
		return payments.PaymentIntent{}, invalidRequest(err)
	}

	var response preferenceResponse
	if err := c.doJSON(
		ctx,
		"create_payment_intent",
		http.MethodPost,
		"/checkout/preferences",
		payload,
		req.IdempotencyKey,
		&response,
	); err != nil {
		return payments.PaymentIntent{}, err
	}

	providerID := string(response.ID)
	intentID := req.TransactionID
	if intentID == "" {
		intentID = providerID
	}
	amount := normalizeMoney(req.Amount, req.Contract.Currency)
	return payments.PaymentIntent{
		ID:          intentID,
		Provider:    ProviderName,
		ProviderID:  providerID,
		Status:      mapPaymentStatus(response.Status, payments.PaymentStatusCreated),
		Amount:      amount,
		CheckoutURL: response.checkoutURL(),
		ExpiresAt:   response.expiresAt(req.ExpiresAt),
		Metadata:    cloneStringMap(req.Metadata),
	}, nil
}

// ConfirmPayment is not part of the Checkout Pro preference flow.
func (c *Client) ConfirmPayment(
	context.Context,
	payments.ConfirmPaymentRequest,
) (payments.Payment, error) {
	return payments.Payment{}, UnsupportedOperationError{Operation: "confirm_payment"}
}

// CapturePayment captures a previously authorized Mercado Pago payment when a
// provider payment id is available.
func (c *Client) CapturePayment(
	ctx context.Context,
	req payments.CapturePaymentRequest,
) (payments.Payment, error) {
	paymentID := providerPaymentID(req.PaymentID, req.ProviderID)
	if paymentID == "" {
		return payments.Payment{}, invalidRequest(ErrPaymentIDMissing)
	}
	if req.Amount.Amount < 0 {
		return payments.Payment{}, invalidRequest(errAmountInvalid)
	}

	payload := capturePayload{Capture: true}
	if req.Amount.Amount > 0 {
		amount := apiAmount(req.Amount.Amount)
		payload.TransactionAmount = &amount
	}

	var response paymentResponse
	if err := c.doJSON(
		ctx,
		"capture_payment",
		http.MethodPut,
		"/v1/payments/"+url.PathEscape(paymentID),
		payload,
		req.IdempotencyKey,
		&response,
	); err != nil {
		return payments.Payment{}, err
	}
	return response.payment(req), nil
}

// RefundPayment creates a full or partial refund for a Mercado Pago payment.
func (c *Client) RefundPayment(
	ctx context.Context,
	req payments.RefundPaymentRequest,
) (payments.Refund, error) {
	paymentID := providerPaymentID(req.PaymentID, req.ProviderID)
	if paymentID == "" {
		return payments.Refund{}, invalidRequest(ErrPaymentIDMissing)
	}
	if req.Amount.Amount < 0 {
		return payments.Refund{}, invalidRequest(errRefundAmountInvalid)
	}
	if strings.TrimSpace(req.IdempotencyKey) == "" {
		return payments.Refund{}, invalidRequest(ErrIdempotencyKeyMissing)
	}

	payload := refundPayload{}
	if req.Amount.Amount > 0 {
		amount := apiAmount(req.Amount.Amount)
		payload.Amount = &amount
	}

	var response refundResponse
	if err := c.doJSON(
		ctx,
		"refund_payment",
		http.MethodPost,
		"/v1/payments/"+url.PathEscape(paymentID)+"/refunds",
		payload,
		req.IdempotencyKey,
		&response,
	); err != nil {
		return payments.Refund{}, err
	}
	return response.refund(req), nil
}

// ParseWebhookEvent is intentionally left to the generated webhook binding.
func (c *Client) ParseWebhookEvent(
	context.Context,
	payments.WebhookRequest,
) (payments.WebhookEvent, error) {
	return payments.WebhookEvent{}, UnsupportedOperationError{Operation: "parse_webhook_event"}
}

func (c *Client) doJSON(
	ctx context.Context,
	operation string,
	method string,
	path string,
	payload any,
	idempotencyKey string,
	out any,
) error {
	if ctx == nil {
		ctx = context.Background()
	}
	if err := c.validate(); err != nil {
		return err
	}
	endpoint, err := c.endpoint(path)
	if err != nil {
		return invalidRequest(fmt.Errorf("%w: %v", ErrBaseURLInvalid, err))
	}

	var body io.Reader
	if payload != nil {
		data, err := json.Marshal(payload)
		if err != nil {
			return invalidRequest(err)
		}
		body = bytes.NewReader(data)
	}

	request, err := http.NewRequestWithContext(ctx, method, endpoint, body)
	if err != nil {
		return invalidRequest(err)
	}
	request.Header.Set("Accept", "application/json")
	request.Header.Set("Authorization", "Bearer "+strings.TrimSpace(c.AccessToken))
	if payload != nil {
		request.Header.Set("Content-Type", "application/json")
	}
	if strings.TrimSpace(idempotencyKey) != "" {
		request.Header.Set(HeaderIdempotencyKey, strings.TrimSpace(idempotencyKey))
	}

	response, err := c.httpClient().Do(request)
	if err != nil {
		if ctxErr := ctx.Err(); ctxErr != nil {
			return ctxErr
		}
		return fmt.Errorf("%w: %w", payments.ErrGatewayUnavailable, err)
	}
	defer response.Body.Close()

	if response.StatusCode < http.StatusOK || response.StatusCode >= http.StatusMultipleChoices {
		body, _ := io.ReadAll(io.LimitReader(response.Body, 4096))
		return providerStatusError(operation, response.StatusCode, strings.TrimSpace(string(body)))
	}
	if out == nil {
		_, _ = io.Copy(io.Discard, response.Body)
		return nil
	}
	if err := json.NewDecoder(response.Body).Decode(out); err != nil {
		if errors.Is(err, io.EOF) {
			return nil
		}
		return fmt.Errorf("%w: mercadopago decode %s response: %w", payments.ErrGatewayUnavailable, operation, err)
	}
	return nil
}

func (c *Client) validate() error {
	if c == nil || strings.TrimSpace(c.AccessToken) == "" {
		return invalidRequest(ErrAccessTokenMissing)
	}
	return nil
}

func (c *Client) endpoint(endpointPath string) (string, error) {
	base := ""
	if c != nil {
		base = strings.TrimSpace(c.BaseURL)
	}
	if base == "" {
		base = DefaultBaseURL
	}
	parsed, err := url.Parse(base)
	if err != nil {
		return "", err
	}
	if parsed.Scheme == "" || parsed.Host == "" {
		return "", ErrBaseURLInvalid
	}
	parsed.Path = strings.TrimRight(parsed.Path, "/") + "/" + strings.TrimLeft(endpointPath, "/")
	parsed.RawQuery = ""
	parsed.Fragment = ""
	return parsed.String(), nil
}

func (c *Client) httpClient() *http.Client {
	if c != nil && c.HTTPClient != nil {
		return c.HTTPClient
	}
	return http.DefaultClient
}

func invalidRequest(err error) error {
	return fmt.Errorf("%w: %w", payments.ErrInvalidPaymentRequest, err)
}

func providerStatusError(operation string, statusCode int, body string) error {
	return &APIError{
		Operation:  operation,
		StatusCode: statusCode,
		Body:       body,
		Err:        statusSentinel(statusCode),
	}
}

func statusSentinel(statusCode int) error {
	switch statusCode {
	case http.StatusBadRequest, http.StatusUnprocessableEntity:
		return payments.ErrInvalidPaymentRequest
	case http.StatusConflict:
		return payments.ErrPaymentIdempotent
	case http.StatusNotFound:
		return payments.ErrPaymentNotFound
	case http.StatusUnauthorized, http.StatusForbidden, http.StatusTooManyRequests:
		return payments.ErrGatewayUnavailable
	default:
		if statusCode >= http.StatusInternalServerError {
			return payments.ErrGatewayUnavailable
		}
		return payments.ErrInvalidPaymentRequest
	}
}

type preferencePayload struct {
	Items             []preferenceItem  `json:"items"`
	Payer             *preferencePayer  `json:"payer,omitempty"`
	ExternalReference string            `json:"external_reference,omitempty"`
	BackURLs          *preferenceURLs   `json:"back_urls,omitempty"`
	NotificationURL   string            `json:"notification_url,omitempty"`
	Expires           bool              `json:"expires,omitempty"`
	ExpirationDateTo  string            `json:"expiration_date_to,omitempty"`
	Metadata          map[string]string `json:"metadata,omitempty"`
}

type preferenceItem struct {
	ID          string    `json:"id,omitempty"`
	Title       string    `json:"title"`
	Description string    `json:"description,omitempty"`
	Quantity    int64     `json:"quantity"`
	CurrencyID  string    `json:"currency_id"`
	UnitPrice   apiAmount `json:"unit_price"`
}

type preferencePayer struct {
	ID             string               `json:"id,omitempty"`
	Email          string               `json:"email,omitempty"`
	Name           string               `json:"name,omitempty"`
	Identification *payerIdentification `json:"identification,omitempty"`
	Metadata       map[string]string    `json:"metadata,omitempty"`
}

type payerIdentification struct {
	Number string `json:"number,omitempty"`
}

type preferenceURLs struct {
	Success string `json:"success,omitempty"`
	Pending string `json:"pending,omitempty"`
	Failure string `json:"failure,omitempty"`
}

func newPreferencePayload(req payments.CreatePaymentIntentRequest) (preferencePayload, error) {
	currency := moneyCurrency(req.Amount, req.Contract.Currency)
	if currency == "" {
		return preferencePayload{}, errCurrencyMissing
	}
	if req.Amount.Amount <= 0 {
		return preferencePayload{}, errAmountInvalid
	}

	items, err := preferenceItems(req, currency)
	if err != nil {
		return preferencePayload{}, err
	}
	payload := preferencePayload{
		Items:             items,
		Payer:             payerPayload(req.Payer),
		ExternalReference: req.TransactionID,
		BackURLs:          backURLs(req),
		NotificationURL:   req.NotificationURL,
		Metadata:          cloneStringMap(req.Metadata),
	}
	if !req.ExpiresAt.IsZero() {
		payload.Expires = true
		payload.ExpirationDateTo = req.ExpiresAt.Format(time.RFC3339)
	}
	return payload, nil
}

func preferenceItems(req payments.CreatePaymentIntentRequest, defaultCurrency string) ([]preferenceItem, error) {
	if len(req.Items) == 0 {
		return []preferenceItem{{
			Title:       preferenceTitle(req.Description, req.TransactionID),
			Description: req.Description,
			Quantity:    1,
			CurrencyID:  defaultCurrency,
			UnitPrice:   apiAmount(req.Amount.Amount),
		}}, nil
	}

	items := make([]preferenceItem, 0, len(req.Items))
	for _, item := range req.Items {
		if item.Quantity <= 0 {
			return nil, errQuantityInvalid
		}
		if item.UnitAmount.Amount <= 0 {
			return nil, errAmountInvalid
		}
		currency := moneyCurrency(item.UnitAmount, defaultCurrency)
		if currency == "" {
			return nil, errCurrencyMissing
		}
		items = append(items, preferenceItem{
			ID:          item.ID,
			Title:       preferenceTitle(item.Title, item.ID),
			Description: item.Description,
			Quantity:    item.Quantity,
			CurrencyID:  currency,
			UnitPrice:   apiAmount(item.UnitAmount.Amount),
		})
	}
	return items, nil
}

func preferenceTitle(primary, fallback string) string {
	if strings.TrimSpace(primary) != "" {
		return strings.TrimSpace(primary)
	}
	if strings.TrimSpace(fallback) != "" {
		return strings.TrimSpace(fallback)
	}
	return "Payment"
}

func payerPayload(payer payments.Payer) *preferencePayer {
	if payer.ID == "" && payer.Email == "" && payer.Name == "" && payer.Document == "" && len(payer.Metadata) == 0 {
		return nil
	}
	payload := &preferencePayer{
		ID:       payer.ID,
		Email:    payer.Email,
		Name:     payer.Name,
		Metadata: cloneStringMap(payer.Metadata),
	}
	if payer.Document != "" {
		payload.Identification = &payerIdentification{Number: payer.Document}
	}
	return payload
}

func backURLs(req payments.CreatePaymentIntentRequest) *preferenceURLs {
	if req.SuccessURL == "" && req.PendingURL == "" && req.FailureURL == "" {
		return nil
	}
	return &preferenceURLs{
		Success: req.SuccessURL,
		Pending: req.PendingURL,
		Failure: req.FailureURL,
	}
}

type capturePayload struct {
	Capture           bool       `json:"capture"`
	TransactionAmount *apiAmount `json:"transaction_amount,omitempty"`
}

type refundPayload struct {
	Amount *apiAmount `json:"amount,omitempty"`
}

type preferenceResponse struct {
	ID               flexibleString  `json:"id"`
	InitPoint        string          `json:"init_point"`
	SandboxInitPoint string          `json:"sandbox_init_point"`
	Status           string          `json:"status"`
	ExpirationDateTo mercadoPagoTime `json:"expiration_date_to"`
	DateOfExpiration mercadoPagoTime `json:"date_of_expiration"`
}

func (r preferenceResponse) checkoutURL() string {
	if r.InitPoint != "" {
		return r.InitPoint
	}
	return r.SandboxInitPoint
}

func (r preferenceResponse) expiresAt(fallback time.Time) time.Time {
	if !r.ExpirationDateTo.IsZero() {
		return r.ExpirationDateTo.Time
	}
	if !r.DateOfExpiration.IsZero() {
		return r.DateOfExpiration.Time
	}
	return fallback
}

type paymentResponse struct {
	ID                flexibleString  `json:"id"`
	Status            string          `json:"status"`
	TransactionAmount apiAmount       `json:"transaction_amount"`
	CurrencyID        string          `json:"currency_id"`
	DateApproved      mercadoPagoTime `json:"date_approved"`
	PaymentMethodID   string          `json:"payment_method_id"`
	PaymentTypeID     string          `json:"payment_type_id"`
}

func (r paymentResponse) payment(req payments.CapturePaymentRequest) payments.Payment {
	providerID := string(r.ID)
	if providerID == "" {
		providerID = req.ProviderID
	}
	paymentID := req.PaymentID
	if paymentID == "" {
		paymentID = providerID
	}
	amount := responseMoney(r.TransactionAmount, r.CurrencyID, normalizeMoney(req.Amount, req.Contract.Currency))
	status := mapCaptureStatus(r.Status)
	payment := payments.Payment{
		ID:            paymentID,
		Provider:      ProviderName,
		ProviderID:    providerID,
		Status:        status,
		Amount:        amount,
		PaymentMethod: paymentMethod(r.PaymentMethodID, r.PaymentTypeID),
		PaidAt:        r.DateApproved.Time,
		Metadata:      cloneStringMap(req.Metadata),
	}
	switch status {
	case payments.PaymentStatusAuthorized:
		payment.AuthorizedAmount = amount
	case payments.PaymentStatusCaptured, payments.PaymentStatusSucceeded:
		payment.CapturedAmount = amount
	}
	return payment
}

type refundResponse struct {
	ID        flexibleString  `json:"id"`
	PaymentID flexibleString  `json:"payment_id"`
	Status    string          `json:"status"`
	Amount    apiAmount       `json:"amount"`
	Reason    string          `json:"reason"`
	CreatedAt mercadoPagoTime `json:"date_created"`
}

func (r refundResponse) refund(req payments.RefundPaymentRequest) payments.Refund {
	providerID := string(r.ID)
	paymentID := req.PaymentID
	if paymentID == "" {
		paymentID = string(r.PaymentID)
	}
	fallbackAmount := normalizeMoney(req.Amount, req.Contract.Currency)
	amount := responseMoney(r.Amount, fallbackAmount.Currency, fallbackAmount)
	reason := req.Reason
	if reason == "" {
		reason = r.Reason
	}
	return payments.Refund{
		ID:         providerID,
		Provider:   ProviderName,
		ProviderID: providerID,
		PaymentID:  paymentID,
		Status:     mapRefundStatus(r.Status),
		Amount:     amount,
		Reason:     reason,
		CreatedAt:  r.CreatedAt.Time,
		Metadata:   cloneStringMap(req.Metadata),
	}
}

type flexibleString string

func (s *flexibleString) UnmarshalJSON(data []byte) error {
	if bytes.Equal(data, []byte("null")) {
		*s = ""
		return nil
	}
	var value string
	if err := json.Unmarshal(data, &value); err == nil {
		*s = flexibleString(value)
		return nil
	}
	*s = flexibleString(strings.TrimSpace(string(data)))
	return nil
}

type apiAmount int64

func (a apiAmount) MarshalJSON() ([]byte, error) {
	return []byte(formatMinorAmount(int64(a))), nil
}

func (a *apiAmount) UnmarshalJSON(data []byte) error {
	amount, err := parseMinorAmount(string(data))
	if err != nil {
		return err
	}
	*a = apiAmount(amount)
	return nil
}

type mercadoPagoTime struct {
	time.Time
}

func (t *mercadoPagoTime) UnmarshalJSON(data []byte) error {
	if bytes.Equal(data, []byte("null")) {
		return nil
	}
	var value string
	if err := json.Unmarshal(data, &value); err != nil {
		return err
	}
	if strings.TrimSpace(value) == "" {
		return nil
	}
	parsed, err := time.Parse(time.RFC3339Nano, value)
	if err != nil {
		return err
	}
	t.Time = parsed
	return nil
}

func formatMinorAmount(amount int64) string {
	sign := ""
	if amount < 0 {
		sign = "-"
		amount = -amount
	}
	whole := amount / 100
	fraction := amount % 100
	if fraction == 0 {
		return sign + strconv.FormatInt(whole, 10)
	}
	return fmt.Sprintf("%s%d.%02d", sign, whole, fraction)
}

func parseMinorAmount(raw string) (int64, error) {
	value := strings.TrimSpace(raw)
	if value == "" || value == "null" {
		return 0, nil
	}
	value = strings.Trim(value, `"`)
	sign := int64(1)
	if strings.HasPrefix(value, "-") {
		sign = -1
		value = strings.TrimPrefix(value, "-")
	}
	whole, fraction, ok := strings.Cut(value, ".")
	if !ok {
		parsed, err := strconv.ParseInt(whole, 10, 64)
		if err != nil {
			return 0, err
		}
		return sign * parsed * 100, nil
	}
	if whole == "" {
		whole = "0"
	}
	parsedWhole, err := strconv.ParseInt(whole, 10, 64)
	if err != nil {
		return 0, err
	}
	for len(fraction) < 2 {
		fraction += "0"
	}
	if len(fraction) > 2 {
		if strings.Trim(fraction[2:], "0") != "" {
			return 0, fmt.Errorf("mercadopago: unsupported amount precision %q", raw)
		}
		fraction = fraction[:2]
	}
	parsedFraction, err := strconv.ParseInt(fraction, 10, 64)
	if err != nil {
		return 0, err
	}
	return sign * (parsedWhole*100 + parsedFraction), nil
}

func moneyCurrency(money payments.Money, fallback string) string {
	if strings.TrimSpace(money.Currency) != "" {
		return strings.TrimSpace(money.Currency)
	}
	return strings.TrimSpace(fallback)
}

func normalizeMoney(money payments.Money, fallbackCurrency string) payments.Money {
	money.Currency = moneyCurrency(money, fallbackCurrency)
	return money
}

func responseMoney(amount apiAmount, currency string, fallback payments.Money) payments.Money {
	if amount == 0 {
		return fallback
	}
	if strings.TrimSpace(currency) == "" {
		currency = fallback.Currency
	}
	return payments.Money{
		Amount:   int64(amount),
		Currency: strings.TrimSpace(currency),
	}
}

func providerPaymentID(paymentID, providerID string) string {
	if strings.TrimSpace(providerID) != "" {
		return strings.TrimSpace(providerID)
	}
	return strings.TrimSpace(paymentID)
}

func paymentMethod(methodID, typeID string) string {
	if methodID != "" {
		return methodID
	}
	return typeID
}

func mapPaymentStatus(status string, fallback payments.PaymentStatus) payments.PaymentStatus {
	switch strings.ToLower(strings.TrimSpace(status)) {
	case "":
		return fallback
	case "pending", "in_process", "in_mediation":
		return payments.PaymentStatusPending
	case "authorized":
		return payments.PaymentStatusAuthorized
	case "approved", "accredited":
		return payments.PaymentStatusSucceeded
	case "rejected":
		return payments.PaymentStatusFailed
	case "cancelled", "canceled":
		return payments.PaymentStatusCanceled
	case "refunded":
		return payments.PaymentStatusRefunded
	default:
		return payments.PaymentStatusUnknown
	}
}

func mapCaptureStatus(status string) payments.PaymentStatus {
	if strings.EqualFold(strings.TrimSpace(status), "approved") {
		return payments.PaymentStatusCaptured
	}
	return mapPaymentStatus(status, payments.PaymentStatusUnknown)
}

func mapRefundStatus(status string) payments.RefundStatus {
	switch strings.ToLower(strings.TrimSpace(status)) {
	case "":
		return payments.RefundStatusUnknown
	case "pending", "in_process":
		return payments.RefundStatusPending
	case "approved", "succeeded":
		return payments.RefundStatusSucceeded
	case "rejected", "failed":
		return payments.RefundStatusFailed
	case "cancelled", "canceled":
		return payments.RefundStatusCanceled
	default:
		return payments.RefundStatusUnknown
	}
}

func cloneStringMap(values map[string]string) map[string]string {
	if len(values) == 0 {
		return nil
	}
	clone := make(map[string]string, len(values))
	for key, value := range values {
		clone[key] = value
	}
	return clone
}
