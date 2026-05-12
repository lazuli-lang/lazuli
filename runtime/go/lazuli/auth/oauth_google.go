package auth

import (
	"context"
	"encoding/json"
	"errors"
	"net/http"

	"golang.org/x/oauth2"
)

// GoogleUserInfo is the OpenID Connect userinfo payload returned by Google.
type GoogleUserInfo struct {
	Sub           string `json:"sub"`
	Email         string `json:"email"`
	EmailVerified bool   `json:"email_verified"`
	Name          string `json:"name"`
	Picture       string `json:"picture"`
}

const googleUserInfoURL = "https://openidconnect.googleapis.com/v1/userinfo"

var errGoogleUserInfoRequestFailed = errors.New("auth: google userinfo request failed")

// GoogleOAuthRedirectURL returns Google's OAuth2 consent URL using the
// provided state token. The caller owns minting and persisting the state.
func GoogleOAuthRedirectURL(contract OAuthContract, state string) (string, error) {
	cfg, err := BuildOAuthConfig(contract)
	if err != nil {
		return "", err
	}
	return cfg.AuthCodeURL(state, oauth2.AccessTypeOnline), nil
}

// GoogleOAuthCallback verifies state, exchanges the authorization code, and
// fetches the authenticated user's OpenID Connect profile from Google.
func GoogleOAuthCallback(
	ctx context.Context,
	contract OAuthContract,
	code, state, cookieState string,
) (*GoogleUserInfo, *oauth2.Token, error) {
	if state == "" || state != cookieState {
		return nil, nil, ErrOAuthStateMismatch
	}
	cfg, err := BuildOAuthConfig(contract)
	if err != nil {
		return nil, nil, err
	}
	token, err := cfg.Exchange(ctx, code)
	if err != nil {
		return nil, nil, err
	}
	client := oauth2.NewClient(ctx, oauth2.StaticTokenSource(token))
	resp, err := client.Get(googleUserInfoURL)
	if err != nil {
		return nil, nil, err
	}
	defer resp.Body.Close()
	if resp.StatusCode != http.StatusOK {
		return nil, nil, errGoogleUserInfoRequestFailed
	}
	var info GoogleUserInfo
	if err := json.NewDecoder(resp.Body).Decode(&info); err != nil {
		return nil, nil, err
	}
	return &info, token, nil
}
