package lazuli

import (
	"net/http"
	"net/http/httptest"
	"os"
	"syscall"
	"testing"
	"time"
)

func TestWaitForShutdownHandlesSimulatedSIGINT(t *testing.T) {
	shutdownCalled := make(chan struct{})
	server := httptest.NewUnstartedServer(http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
		w.WriteHeader(http.StatusNoContent)
	}))
	server.Config.RegisterOnShutdown(func() {
		close(shutdownCalled)
	})
	server.Start()
	t.Cleanup(server.Close)

	sigs := make(chan os.Signal, 1)
	errc := make(chan error, 1)
	go func() {
		errc <- waitForShutdown(server.Config, time.Second, sigs)
	}()

	sigs <- syscall.SIGINT

	select {
	case <-shutdownCalled:
	case <-time.After(2 * time.Second):
		t.Fatal("Shutdown was not called after SIGINT")
	}

	select {
	case err := <-errc:
		if err != nil {
			t.Fatalf("WaitForShutdown returned %v, want nil", err)
		}
	case <-time.After(2 * time.Second):
		t.Fatal("WaitForShutdown did not return")
	}
}
