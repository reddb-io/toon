# MIT license, deviating from the org's BUSL-1.1 default

reddb.io's flagship (`reddb`) ships under BUSL-1.1 to guard against cloud providers reselling the database as a service. `tq` is licensed MIT instead: it is a local CLI query tool whose only competitive currency against entrenched incumbents (jq and yq, both MIT) is frictionless adoption — by users, contributors, and distro packagers (Homebrew, nixpkgs, Debian reject BUSL) — and the resell-as-a-service scenario BUSL protects against does not exist for a local binary. Applies to both workspace crates (`reddb-io-toon`, `reddb-io-tq`).
