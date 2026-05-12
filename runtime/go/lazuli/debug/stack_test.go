package debug

import (
	"reflect"
	"testing"
)

func TestParseStackExtractsGoPanicFrames(t *testing.T) {
	stack := []byte("panic: boom\n\n" +
		"goroutine 19 [running]:\n" +
		"lazuli.dev/app/features/customer.HandleCreateCustomer({0x1, 0x2})\n" +
		"\tfeatures/customer.lzi:42 +0x1a\n" +
		"lazuli.dev/runtime/lazuli.RunCommand(...)\n" +
		"\tC:/work/runtime/go/lazuli/run.go:55 +0x42\n" +
		"created by testing.(*T).Run in goroutine 1\n" +
		"\tC:\\Go\\src\\testing\\testing.go:1999 +0x465\n")

	got := ParseStack(stack)
	want := []StackFrame{
		{
			Function: "lazuli.dev/app/features/customer.HandleCreateCustomer",
			File:     "features/customer.lzi",
			Line:     42,
			LZI:      true,
		},
		{
			Function: "lazuli.dev/runtime/lazuli.RunCommand",
			File:     "C:/work/runtime/go/lazuli/run.go",
			Line:     55,
		},
		{
			Function: "testing.(*T).Run",
			File:     "C:\\Go\\src\\testing\\testing.go",
			Line:     1999,
		},
	}
	if !reflect.DeepEqual(got, want) {
		t.Fatalf("ParseStack = %#v, want %#v", got, want)
	}
}

func TestParseStackHandlesReceiverFunctionsAndOptionalColumn(t *testing.T) {
	stack := []byte("goroutine 1 [running]:\r\n" +
		"github.com/acme/app.(*Handler).Serve(0xc0000120c0, {0x1, 0x2})\r\n" +
		"\tC:\\work\\features\\order.lzi:108:7 +0x2f\r\n")

	got := ParseStack(stack)
	want := []StackFrame{
		{
			Function: "github.com/acme/app.(*Handler).Serve",
			File:     "C:\\work\\features\\order.lzi",
			Line:     108,
			Column:   7,
			LZI:      true,
		},
	}
	if !reflect.DeepEqual(got, want) {
		t.Fatalf("ParseStack = %#v, want %#v", got, want)
	}
}

func TestParseStackIgnoresMalformedAndPartialFrames(t *testing.T) {
	stack := []byte("goroutine 1 [running]:\n" +
		"not a frame\n" +
		"\tfeatures/customer.lzi:42 +0x1a\n" +
		"main.good()\n" +
		"\tfeatures/customer.lzi:not-a-line +0x1a\n" +
		"main.next()\n" +
		"\tfeatures/customer.lzi:7 +0x1a\n")

	got := ParseStack(stack)
	want := []StackFrame{
		{
			Function: "main.next",
			File:     "features/customer.lzi",
			Line:     7,
			LZI:      true,
		},
	}
	if !reflect.DeepEqual(got, want) {
		t.Fatalf("ParseStack = %#v, want %#v", got, want)
	}
}

func TestParseStackReturnsNilForNoFrames(t *testing.T) {
	if got := ParseStack(nil); got != nil {
		t.Fatalf("ParseStack(nil) = %#v, want nil", got)
	}
	if got := ParseStack([]byte("panic: boom\n")); got != nil {
		t.Fatalf("ParseStack(no frames) = %#v, want nil", got)
	}
}
