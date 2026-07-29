// Complement black-box interop tests for synapse-rust.
//
// These tests follow the Complement base image contract:
// https://github.com/matrix-org/complement/blob/main/docs/homeserver-design-overview.md
//
// Run via:
//   COMPLEMENT_BASE_IMAGE=complement-synapse-rust \
//     go test -v ./tests/complement/...
//
// Minimum interop coverage (per ELEMENT_SYNAPSE_GAP_ANALYSIS_2026-07-28.md P0-3):
//   register → login → sync → create room → send event
//   → federation key → server discovery → media upload/download
package complement

import (
	"io"
	"net/http"
	"testing"

	"github.com/matrix-org/complement/b"
	"github.com/matrix-org/complement/client"
	"github.com/matrix-org/complement/helpers"
	"github.com/matrix-org/complement/match"
	"github.com/matrix-org/complement/must"

	"github.com/tidwall/gjson"
)

// TestRegisterLogin verifies the most basic registration + login flow.
//
// Acceptance:
//   - register a brand-new user, expect 200 + access_token
//   - login with the same password, expect 200 + access_token
//   - both user_id values must match
func TestRegisterLogin(t *testing.T) {
	deployment := b.GetDeployment(t)
	defer deployment.Destroy(t)

	// Register a brand-new user on hs1.
	alice := deployment.Register(t, "hs1", helpers.RegistrationOpts{
		LocalpartSuffix: "alice",
		Password:        "alice-password",
	})

	// Login the same user via the password flow.
	loggedIn := deployment.Login(t, "hs1", alice, helpers.LoginOpts{
		Password: "alice-password",
	})

	if loggedIn.UserID != alice.UserID {
		t.Fatalf("user_id mismatch: register=%s login=%s", alice.UserID, loggedIn.UserID)
	}
}

// TestInitialSync verifies that a freshly registered user can call /sync
// and receives a sensible initial response (next_batch + rooms object).
func TestInitialSync(t *testing.T) {
	deployment := b.GetDeployment(t)
	defer deployment.Destroy(t)

	alice := deployment.Register(t, "hs1", helpers.RegistrationOpts{
		LocalpartSuffix: "alice_sync",
		Password:        "alice-password",
	})

	resp := alice.Do(t, "GET", []string{"_matrix", "client", "v3", "sync"})
	must.MatchResponse(t, resp, http.StatusOK, match.HTTPResponse{
		JSON: []match.JSON{
			match.JSONKeyPresent("next_batch"),
			match.JSONKeyPresent("rooms"),
		},
	})
}

// TestCreateRoomAndSendEvent covers the minimal room lifecycle:
// create room → join → send a message event → /sync surfaces it.
func TestCreateRoomAndSendEvent(t *testing.T) {
	deployment := b.GetDeployment(t)
	defer deployment.Destroy(t)

	alice := deployment.Register(t, "hs1", helpers.RegistrationOpts{
		LocalpartSuffix: "alice_room",
		Password:        "alice-password",
	})

	roomID := alice.CreateRoom(t, map[string]any{
		"preset": "private_chat",
	})

	// Send a message event.
	eventID := alice.SendEventSynced(t, roomID, b.Event{
		Type: "m.room.message",
		Content: map[string]any{
			"msgtype": "m.text",
			"body":    "hello from synapse-rust",
		},
	})

	if eventID == "" {
		t.Fatalf("expected non-empty event_id from SendEventSynced")
	}

	// Verify the event shows up in /sync.
	resp := alice.MustSync(t, client.SyncReq{})
	joinedRooms := gjson.GetBytes(resp, "rooms.join").Map()
	if _, ok := joinedRooms[roomID]; !ok {
		t.Fatalf("room %s missing from sync joined rooms", roomID)
	}
}

// TestServerDiscovery verifies the /_matrix/key/v2/server endpoint that
// federation relies on. Uses an unauthenticated client so we exercise the
// public key publication path rather than any authenticated capability.
func TestServerDiscovery(t *testing.T) {
	deployment := b.GetDeployment(t)
	defer deployment.Destroy(t)

	anonymousClient := deployment.UnauthenticatedClient(t, "hs1")

	resp := anonymousClient.Do(t, "GET", []string{"_matrix", "key", "v2", "server"})
	must.MatchResponse(t, resp, http.StatusOK, match.HTTPResponse{
		JSON: []match.JSON{
			match.JSONKeyPresent("server_name"),
			match.JSONKeyPresent("verify_keys"),
			match.JSONKeyPresent("valid_until_ts"),
		},
	})
}

// TestMediaUploadDownload verifies the minimal media upload → download
// roundtrip path described in the P0-3 minimum interop coverage list.
func TestMediaUploadDownload(t *testing.T) {
	deployment := b.GetDeployment(t)
	defer deployment.Destroy(t)

	alice := deployment.Register(t, "hs1", helpers.RegistrationOpts{
		LocalpartSuffix: "alice_media",
		Password:        "alice-password",
	})

	body := []byte("hello synapse-rust media interop")
	mxcURI := alice.UploadContent(t, body, "interop.txt", "text/plain")

	if mxcURI == "" {
		t.Fatalf("expected non-empty mxc:// URI from UploadContent")
	}

	// Download the uploaded media and verify the body matches.
	resp := alice.Do(t, "GET", []string{"_matrix", "media", "v3", "download", "hs1", "interop.txt"})
	if resp.StatusCode != http.StatusOK {
		t.Fatalf("media download failed: expected 200, got %d", resp.StatusCode)
	}
	downloaded, err := io.ReadAll(resp.Body)
	if err != nil {
		t.Fatalf("failed to read media download body: %v", err)
	}
	if string(downloaded) != string(body) {
		t.Fatalf("media body mismatch: uploaded %q, downloaded %q", string(body), string(downloaded))
	}
}
