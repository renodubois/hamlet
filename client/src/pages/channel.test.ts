import { describe, expect, test } from "vitest";
import { syncMessagesForCurrentUser } from "./channel";
import type { Message, User } from "../api";

describe("syncMessagesForCurrentUser", () => {
  test("updates existing messages to use the latest current-user profile", () => {
    const messages: Message[] = [
      {
        id: 1,
        user_id: 7,
        channel_id: 100,
        text: "before rename",
        username: "alice",
        display_name: null,
        avatar_url: null,
      },
      {
        id: 2,
        user_id: 9,
        channel_id: 100,
        text: "someone else",
        username: "bob",
        display_name: null,
        avatar_url: null,
      },
    ];

    const user: User = {
      id: 7,
      username: "alice",
      display_name: "Ally",
      email: null,
      email_verified: false,
      avatar_url: "/uploads/avatars/7.webp?v=2",
    };

    expect(syncMessagesForCurrentUser(messages, user)).toEqual([
      {
        id: 1,
        user_id: 7,
        channel_id: 100,
        text: "before rename",
        username: "alice",
        display_name: "Ally",
        avatar_url: "/uploads/avatars/7.webp?v=2",
      },
      {
        id: 2,
        user_id: 9,
        channel_id: 100,
        text: "someone else",
        username: "bob",
        display_name: null,
        avatar_url: null,
      },
    ]);
  });
});
