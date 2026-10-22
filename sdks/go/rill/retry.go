package rill

import (
	"context"
	"errors"
	"math/rand/v2"
	"time"

	"connectrpc.com/connect"
)

// RetryBudget is how long Retry keeps trying: past a dead server's
// idempotency claim (15 s on the server).
const RetryBudget = 20 * time.Second

// Retry runs call until it succeeds, fails for good, or the budget runs
// out. It retries while the cluster is unavailable and while the server
// refuses for now with a wait (a rate limit, a full queue), waiting what
// the server asks. Retrying is safe for reads and for mutations carrying a
// request_id, which the server runs at most once.
func Retry(ctx context.Context, call func(context.Context) error) error {
	deadline := time.Now().Add(RetryBudget)
	delay := 100 * time.Millisecond
	for {
		err := call(ctx)
		wait, again := retryable(err)
		if !again {
			return err
		}
		if wait == 0 {
			wait = delay/2 + rand.N(delay/2+1)
			delay = min(delay*2, 2*time.Second)
		}
		wait = min(wait, 5*time.Second)
		if time.Now().Add(wait).After(deadline) {
			return err
		}
		select {
		case <-ctx.Done():
			return err
		case <-time.After(wait):
		}
	}
}

func retryable(err error) (time.Duration, bool) {
	var ce *connect.Error
	if err == nil || !errors.As(err, &ce) {
		return 0, false
	}
	switch ce.Code() {
	case connect.CodeUnavailable:
		return 0, true
	case connect.CodeResourceExhausted:
		return RetryAfter(err)
	}
	return 0, false
}
