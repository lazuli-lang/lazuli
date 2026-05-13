package email

import (
	"bufio"
	"bytes"
	"errors"
	"fmt"
	"io"
	"net"
	"strings"
	"sync"
)

const defaultSMTPDevMaxMessageBytes int64 = 10 << 20

var (
	// ErrSMTPDevServerProtocol is wrapped by malformed SMTP client commands.
	ErrSMTPDevServerProtocol = errors.New("email: smtp dev server protocol error")
)

// SMTPDevMessage is one message captured by SMTPDevServer.
type SMTPDevMessage struct {
	HELO string
	From string
	To   []string
	Data []byte
}

// SMTPDevServer is a small, plaintext SMTP capture server for local
// development and tests. It intentionally does not implement TLS or AUTH.
type SMTPDevServer struct {
	MaxMessageBytes int64
	OnMessage       func(SMTPDevMessage)

	mu       sync.Mutex
	messages []SMTPDevMessage
}

// NewSMTPDevServer returns an empty development SMTP capture server.
func NewSMTPDevServer() *SMTPDevServer {
	return &SMTPDevServer{}
}

// Serve accepts SMTP clients from listener until listener is closed or an
// accept error occurs.
func (s *SMTPDevServer) Serve(listener net.Listener) error {
	for {
		conn, err := listener.Accept()
		if err != nil {
			if errors.Is(err, net.ErrClosed) {
				return nil
			}
			return err
		}

		go func() {
			_ = s.serveConn(conn)
		}()
	}
}

// Messages returns a snapshot of captured messages.
func (s *SMTPDevServer) Messages() []SMTPDevMessage {
	s.mu.Lock()
	defer s.mu.Unlock()

	messages := make([]SMTPDevMessage, len(s.messages))
	for i, message := range s.messages {
		messages[i] = cloneSMTPDevMessage(message)
	}
	return messages
}

// Reset clears captured messages.
func (s *SMTPDevServer) Reset() {
	s.mu.Lock()
	defer s.mu.Unlock()
	s.messages = nil
}

func (s *SMTPDevServer) serveConn(conn net.Conn) error {
	defer conn.Close()

	reader := bufio.NewReader(conn)
	writer := bufio.NewWriter(conn)
	session := smtpDevSession{}

	if err := smtpDevWriteLine(writer, "220 lazuli smtp dev server"); err != nil {
		return err
	}

	for {
		line, err := smtpDevReadLine(reader)
		if err != nil {
			if errors.Is(err, io.EOF) {
				return nil
			}
			return err
		}
		command, arg := smtpDevSplitCommand(line)

		switch command {
		case "HELO":
			if strings.TrimSpace(arg) == "" {
				if err := smtpDevWriteLine(writer, "501 HELO requires a name"); err != nil {
					return err
				}
				continue
			}
			session.resetEnvelope()
			session.helo = strings.TrimSpace(arg)
			if err := smtpDevWriteLine(writer, "250 OK"); err != nil {
				return err
			}
		case "EHLO":
			if strings.TrimSpace(arg) == "" {
				if err := smtpDevWriteLine(writer, "501 EHLO requires a name"); err != nil {
					return err
				}
				continue
			}
			session.resetEnvelope()
			session.helo = strings.TrimSpace(arg)
			if err := smtpDevWriteLine(writer, "250-lazuli smtp dev server"); err != nil {
				return err
			}
			if err := smtpDevWriteLine(writer, "250 OK"); err != nil {
				return err
			}
		case "MAIL":
			from, err := smtpDevParsePathArg(arg, "FROM")
			if err != nil {
				if err := smtpDevWriteLine(writer, "501 "+err.Error()); err != nil {
					return err
				}
				continue
			}
			session.resetEnvelope()
			session.from = from
			if err := smtpDevWriteLine(writer, "250 OK"); err != nil {
				return err
			}
		case "RCPT":
			if session.from == "" {
				if err := smtpDevWriteLine(writer, "503 MAIL FROM required before RCPT TO"); err != nil {
					return err
				}
				continue
			}
			to, err := smtpDevParsePathArg(arg, "TO")
			if err != nil {
				if err := smtpDevWriteLine(writer, "501 "+err.Error()); err != nil {
					return err
				}
				continue
			}
			session.to = append(session.to, to)
			if err := smtpDevWriteLine(writer, "250 OK"); err != nil {
				return err
			}
		case "DATA":
			if session.from == "" || len(session.to) == 0 {
				if err := smtpDevWriteLine(writer, "503 MAIL FROM and RCPT TO required before DATA"); err != nil {
					return err
				}
				continue
			}
			if strings.TrimSpace(arg) != "" {
				if err := smtpDevWriteLine(writer, "501 DATA does not accept arguments"); err != nil {
					return err
				}
				continue
			}
			if err := smtpDevWriteLine(writer, "354 End data with <CR><LF>.<CR><LF>"); err != nil {
				return err
			}
			data, err := smtpDevReadData(reader, s.maxMessageBytes())
			if err != nil {
				if errors.Is(err, ErrSMTPDevServerProtocol) {
					if writeErr := smtpDevWriteLine(writer, "552 "+err.Error()); writeErr != nil {
						return writeErr
					}
					session.resetEnvelope()
					continue
				}
				return err
			}
			s.capture(SMTPDevMessage{
				HELO: session.helo,
				From: session.from,
				To:   append([]string(nil), session.to...),
				Data: data,
			})
			session.resetEnvelope()
			if err := smtpDevWriteLine(writer, "250 OK"); err != nil {
				return err
			}
		case "RSET":
			session.resetEnvelope()
			if err := smtpDevWriteLine(writer, "250 OK"); err != nil {
				return err
			}
		case "NOOP":
			if err := smtpDevWriteLine(writer, "250 OK"); err != nil {
				return err
			}
		case "QUIT":
			if err := smtpDevWriteLine(writer, "221 Bye"); err != nil {
				return err
			}
			return nil
		case "":
			if err := smtpDevWriteLine(writer, "500 empty command"); err != nil {
				return err
			}
		default:
			if err := smtpDevWriteLine(writer, "500 command not recognized"); err != nil {
				return err
			}
		}
	}
}

func (s *SMTPDevServer) maxMessageBytes() int64 {
	if s.MaxMessageBytes <= 0 {
		return defaultSMTPDevMaxMessageBytes
	}
	return s.MaxMessageBytes
}

func (s *SMTPDevServer) capture(message SMTPDevMessage) {
	message = cloneSMTPDevMessage(message)

	s.mu.Lock()
	s.messages = append(s.messages, message)
	callback := s.OnMessage
	s.mu.Unlock()

	if callback != nil {
		callback(cloneSMTPDevMessage(message))
	}
}

type smtpDevSession struct {
	helo string
	from string
	to   []string
}

func (s *smtpDevSession) resetEnvelope() {
	s.from = ""
	s.to = nil
}

func smtpDevReadLine(reader *bufio.Reader) (string, error) {
	line, err := reader.ReadString('\n')
	if err != nil {
		return "", err
	}
	return strings.TrimRight(line, "\r\n"), nil
}

func smtpDevWriteLine(writer *bufio.Writer, line string) error {
	if _, err := writer.WriteString(line + "\r\n"); err != nil {
		return err
	}
	return writer.Flush()
}

func smtpDevSplitCommand(line string) (string, string) {
	line = strings.TrimLeft(line, " \t")
	if line == "" {
		return "", ""
	}
	command, arg, ok := strings.Cut(line, " ")
	if !ok {
		return strings.ToUpper(command), ""
	}
	return strings.ToUpper(command), strings.TrimLeft(arg, " \t")
}

func smtpDevParsePathArg(arg, keyword string) (string, error) {
	prefix := keyword + ":"
	if len(arg) < len(prefix) || !strings.EqualFold(arg[:len(prefix)], prefix) {
		return "", fmt.Errorf("%s requires %s:<address>", keyword, keyword)
	}
	value := strings.TrimSpace(arg[len(prefix):])
	if !strings.HasPrefix(value, "<") {
		return "", fmt.Errorf("%s address must be enclosed in angle brackets", keyword)
	}
	end := strings.Index(value, ">")
	if end < 0 {
		return "", fmt.Errorf("%s address is missing closing angle bracket", keyword)
	}
	address := value[1:end]
	if strings.TrimSpace(value[end+1:]) != "" {
		return "", fmt.Errorf("%s parameters are not supported", keyword)
	}
	if err := ValidateAddress(Address{Email: address}); err != nil {
		return "", fmt.Errorf("%s address is invalid: %v", keyword, err)
	}
	return address, nil
}

func smtpDevReadData(reader *bufio.Reader, maxBytes int64) ([]byte, error) {
	var data bytes.Buffer
	var exceeded bool
	for {
		line, err := reader.ReadString('\n')
		if err != nil {
			return nil, err
		}
		trimmed := strings.TrimRight(line, "\r\n")
		if trimmed == "." {
			if exceeded {
				return nil, fmt.Errorf("%w: message exceeds %d bytes", ErrSMTPDevServerProtocol, maxBytes)
			}
			return data.Bytes(), nil
		}
		if exceeded {
			continue
		}
		if strings.HasPrefix(line, "..") {
			line = line[1:]
		}
		line = strings.TrimRight(line, "\r\n") + "\r\n"
		if int64(data.Len()+len(line)) > maxBytes {
			exceeded = true
			continue
		}
		if _, err := data.WriteString(line); err != nil {
			return nil, err
		}
	}
}

func cloneSMTPDevMessage(message SMTPDevMessage) SMTPDevMessage {
	message.To = append([]string(nil), message.To...)
	message.Data = append([]byte(nil), message.Data...)
	return message
}
