export const CHANNEL_NAME_MAX_LEN = 128;
export const DISPLAY_NAME_MAX_LEN = 64;

// How often the client POSTs /typing while the user is typing. Smaller =
// snappier indicator in other connected sessions, more server traffic.
//
// INVARIANT: TYPING_PING_INTERVAL_MS < TYPING_EXPIRY_MS — otherwise the
// indicator would flicker between pings.
export const TYPING_PING_INTERVAL_MS = 2000;

// How long after a user's last typing ping the indicator keeps showing them.
export const TYPING_EXPIRY_MS = 2500;
