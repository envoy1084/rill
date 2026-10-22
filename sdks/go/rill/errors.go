package rill

import (
	"errors"
	"time"

	"connectrpc.com/connect"
	"google.golang.org/genproto/googleapis/rpc/errdetails"
)

// ErrorDomain is the ErrorInfo domain of Rill's error reasons.
const ErrorDomain = "rill.dev"

// Reason is the error's reason (NOT_FOUND, ETAG_MISMATCH, …), or "".
func Reason(err error) string {
	for _, d := range details(err) {
		if info, ok := d.(*errdetails.ErrorInfo); ok && info.GetDomain() == ErrorDomain {
			return info.GetReason()
		}
	}
	return ""
}

// Is reports whether err is a refusal for reason.
func Is(err error, reason string) bool {
	return Reason(err) == reason
}

// Missing lists what a policy denial still needs.
func Missing(err error) []string {
	var out []string
	for _, d := range details(err) {
		if failure, ok := d.(*errdetails.PreconditionFailure); ok {
			for _, v := range failure.GetViolations() {
				out = append(out, v.GetDescription())
			}
		}
	}
	return out
}

// RetryAfter is how long the server asked the caller to wait.
func RetryAfter(err error) (time.Duration, bool) {
	for _, d := range details(err) {
		if retry, ok := d.(*errdetails.RetryInfo); ok {
			return retry.GetRetryDelay().AsDuration(), true
		}
	}
	return 0, false
}

func details(err error) []any {
	var ce *connect.Error
	if !errors.As(err, &ce) {
		return nil
	}
	var out []any
	for _, d := range ce.Details() {
		if v, e := d.Value(); e == nil {
			out = append(out, v)
		}
	}
	return out
}
