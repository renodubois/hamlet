# Hamlet, chat for you and your friends

Hamlet is a self-hosted chat application intended for small communities of friends, families, and other groups that have high implicit trust with each other. It has features you'd expect from something like Discord (text channels, voice & video chats, screen sharing, etc.) while leaving out things that might not be needed, such as moderation tools, large community management, etc.

We don't have any paywalls, or any sort of premium features, but also don't host any Hamlet instances ourselves. The intention is for you to self-host a server for your community, and you're free to modify the server & client implementations (or write your own!) to work best for your needs. Although self-hosting is more work than just using a hosted service, you're able to know that the application will never try and sell your data, display ads, lock new features behind a paywall, or ask you to verify your age. **This should be software that you can feel comfortable relying on for the long-term.**

Hamlet is a work-in-progress right now, and isn't intended for general use yet. The server & client implemetations could change at any time.

More documentation (and a proper website) will be available in the future. Issues are currently limited to contributors but if you have questions about the project, feel free to [send me an email](mailto:me@renodubois.com).

## Technical Details
As of Oct 2026, the server is written in Rust using Actix Web. The desktop client is also written in Rust, using GPUI. Mobile clients will also be available in the future (not sure if they'll be strictly native or use something else). Voice/video chat & screen sharing are being provided via LiveKit.

### LLMs are being used to help develop this project.

Hamlet is something that we are planning on using ourselves, so code quality is still important! This isn't a one-shotted slop repository, we care about what is getting run on our machines, and we care about what is getting run on yours! We're not super experienced Rust developers, so there's a good chance some of the code isn't the best Rust code you've ever seen, but that can always be improved in the future.

The documentation however, is written by hand, by a human. Anything you see in `.agents/` or `llm-docs/` is likely LLM-generated, and the intended consumer is other agents. Things like `README.md` (including this one!) or other docs added later, will be human-generated, and intended for consumption by humans.
