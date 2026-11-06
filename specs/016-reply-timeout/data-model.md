# Data Model: Reply Deadline

No outcome changes: a deadline that passes is the existing `reply_timeout` (`uncertain` for the writes, `error` for `order info`).

## Discovery: `global_options`

One object per option accepted at any command depth, sorted by name:

| Key | Meaning |
| --- | --- |
| `name` | the long option name, without dashes (`output`, `reply-timeout`) |
| `type` | the value type on the command line (`string`, as for the command options) |
| `description` | the help text, with the range and default |
| `enum` | the allowed values, when the option has a fixed set (`output`) |
| `default` | the default value as text, when there is one (`"15"` for `reply-timeout`) |
| `applies_to` | the canonical command names the option takes effect on, sorted |
