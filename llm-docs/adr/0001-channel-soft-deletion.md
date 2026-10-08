# Retain deleted channels and their conversations

Channel deletion marks the channel as deleted rather than physically removing it or its messages, preserving the possibility of reversible deletion in the future. Deleted channels are absent from normal channel lists, and history reads, message sends, renames, and repeat deletes return not found; this change introduces no restoration API. Retaining inaccessible conversation data is deliberate, not an incomplete cascade deletion.

Channel names are unique case-insensitively among active channels only, so a deleted name may be reused for a new channel with its own identity and conversation. A future restoration feature must resolve name conflicts rather than transferring messages to the new channel.
