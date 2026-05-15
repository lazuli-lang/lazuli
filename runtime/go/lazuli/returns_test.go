package lazuli

import (
	"errors"
	"testing"
)

type returnsInput struct {
	Name string
}

type returnsOutput struct {
	Message string
}

func returnsAuthenticatedCtx() *Ctx {
	return &Ctx{
		Actor: ActorUser,
		User:  &User{ID: 1},
	}
}

func returnsPolicy() Policy {
	return Policy{
		Name: "@policy.authenticated",
		Atoms: []PolicyAtom{
			{Namespace: "predicate", Name: "authenticated"},
		},
	}
}

func TestReturnsEffectHappyPath(t *testing.T) {
	cmd := Command[returnsInput, returnsOutput]{
		Name:   "customer.summary",
		Policy: returnsPolicy(),
		Effect: Returns(func(ctx *Ctx, input returnsInput) (returnsOutput, error) {
			if ctx.User == nil {
				t.Fatal("expected context to reach returns handler")
			}
			return returnsOutput{Message: "hello " + input.Name}, nil
		}),
	}

	out, err := cmd.Handle(returnsAuthenticatedCtx(), returnsInput{Name: "Ada"})
	if err != nil {
		t.Fatalf("Handle returned error: %v", err)
	}
	if out.Message != "hello Ada" {
		t.Fatalf("Message = %q, want %q", out.Message, "hello Ada")
	}
}

func TestReturnsEffectWrongTypeInput(t *testing.T) {
	eff := Returns(func(ctx *Ctx, input returnsInput) (returnsOutput, error) {
		return returnsOutput{Message: input.Name}, nil
	})

	_, err := applyReturns[string, returnsOutput](&Ctx{}, eff, "wrong")
	var le *Error
	if !errors.As(err, &le) {
		t.Fatalf("expected *Error, got %T: %v", err, err)
	}
	if le.Status != 500 || le.Code != CodeInternal {
		t.Fatalf("error = (%d, %s), want (500, %s)", le.Status, le.Code, CodeInternal)
	}
	if le.Message != "returns handler received input of wrong type" {
		t.Fatalf("Message = %q", le.Message)
	}
}

func TestReturnsEffectPropagatesHandlerError(t *testing.T) {
	sentinel := errors.New("summary failed")
	eff := Returns(func(ctx *Ctx, input returnsInput) (returnsOutput, error) {
		return returnsOutput{}, sentinel
	})

	_, err := applyReturns[returnsInput, returnsOutput](&Ctx{}, eff, returnsInput{Name: "Ada"})
	if !errors.Is(err, sentinel) {
		t.Fatalf("err = %v, want sentinel", err)
	}
}
