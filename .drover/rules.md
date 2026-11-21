# Rules

At most 30 "don't X" lines, each naming its enforcer or `none`.

- Don't unwrap, expect, panic, index unchecked or `as`-cast in cli/src: clippy (cli/Cargo.toml [lints]).
- Don't println!/eprintln! outside cli/src/out.rs: clippy print_stdout, print_stderr (cli/Cargo.toml [lints]).
- Don't write `unsafe` outside cli/sys: rustc unsafe_code = "forbid" (cli/Cargo.toml [lints]).
- Don't add a crate with a license, source or second version deny.toml refuses: cargo deny (cli/deny.toml).
- Don't force-unwrap, try! or as! in app sources: swiftlint (.swiftlint.yml).
- Don't fatalError( or print( in app sources: swiftlint custom rules (.swiftlint.yml).
- Don't leave a Swift warning: SWIFT_TREAT_WARNINGS_AS_ERRORS (app/Beacon.xcodeproj), -warnings-as-errors (ci.yml swift job).
- Don't pad a screen body (ScrollView, sheet) with literal .padding; use .screenBody() (enforced by: screen-body `sh scripts/lessons/screen-body.sh`, shadow since 2026-11-21).
