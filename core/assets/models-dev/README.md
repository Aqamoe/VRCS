# Public model metadata

`catalog.json` contains only supported provider IDs and model capability fields
from [models.dev](https://models.dev/). `provenance.json` records the download time,
ETag, and SHA-256 of the upstream response. Keep `LICENSE` with these files.

To update the bundle, run `./scripts/update-models-dev.ps1` from the repository
root in PowerShell. Review the data changes before release. Builds use the saved
snapshot and do not download the catalog.

The runtime cache is `models-dev-cache.json` beside the application configuration.
It contains public metadata only. Provider connection settings and credentials
are not part of this cache. Provider API fields take priority over public fields.
Model matching uses exact IDs within the mapped provider and region.

The process shares one store. Startup loads the bundle and a newer valid cache.
Configured cloud profiles can trigger a background refresh. Successful checks
have a 24-hour interval; failed checks have a one-hour retry interval. Generation
reads memory and does not wait for this refresh. Data older than seven days is
marked stale and cannot add public request parameters or output limits.

Request adaptation is limited to implemented provider protocols. Explicit effort
and token-budget settings are validated against current model controls before
the request is sent. Budgets must leave room for an answer within the final
output limit. Gemini dynamic budgets require confirmed support. OpenRouter
budgets require its native `supports_max_tokens` declaration. Native fields and
verified built-in protocol rules take priority; stale public control fields
cannot authorize explicit settings. Unknown and custom endpoints retain their
existing default behavior and reject unconfirmed explicit settings. Tool and
structured-output metadata do not enable new request features.
