# Domain glossary

- **Dictation Session**: The recording-to-ASR lifecycle initiated by the global shortcut or desktop controls.
- **Transcript**: Text returned by a successful ASR process after trailing newlines are removed.
- **Transcript Entry**: A durable history record containing transcript text, creation timestamp, and internal identifier.
- **Transcript History**: The indefinitely retained, local collection of Transcript Entries, ordered newest first.
- **Last Transcript**: The newest Transcript Entry by creation timestamp and identifier.
- **Successful Transcript**: A nonempty transcript other than the placeholder `...` from a session that was not cancelled and whose ASR process exited successfully.
- **Clear History**: The confirmed destructive action that permanently removes all Transcript Entries.
