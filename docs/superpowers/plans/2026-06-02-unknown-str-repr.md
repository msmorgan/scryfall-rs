# Unknown-`&'static str` Repr — Branch 1 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the `unknown_variants` fallback payload `Unknown(Box<str>)` with an interned, `Copy` `Unknown(&'static str)` across the six already-netted enums, behind a single `unknown_fallback!` macro.

**Architecture:** A feature-gated `src/unknown.rs` provides a tiny leak+dedup string interner and a `serde(with)` codec that turns any string into a `&'static str`. A crate-internal `unknown_fallback!` macro (in `src/macros.rs`) emits each enum with its derives, the `non_exhaustive`/`Unknown` cfg-dance, and the interned `Unknown` arm. The six enums are rewritten as macro invocations. Default and slim builds are unaffected; `unknown_variants` builds regain `Copy`.

**Tech Stack:** Rust, `serde` (per-variant `#[serde(untagged)]` + field `#[serde(with)]`), `once_cell::sync::Lazy`, `static_assertions`.

**Scope guard:** Branch 1 only. Do **not** add new enums to the net (Branch 3) and do **not** edit CI workflows, `scripts/test-features.sh`, or `tests/variants-feature/*` (Branch 2). The existing `tests/variants-feature/unknown_variants.rs` already constructs `Unknown("foo".into())` and asserts `assert_eq_size!(_, [u8; 24])`; both remain valid for `&'static str` (same 16-byte layout as `Box<str>`; `"foo".into()` is identity for `&'static str`; equality is by content), so that file is the untouched regression net.

---

## File Structure

- **Create `src/unknown.rs`** — interner (`intern`) + `interned` serde codec. Compiled only under `feature = "unknown_variants"`. Owns the interner unit tests and the `Copy` static-assertions.
- **Create `src/macros.rs`** — the `unknown_fallback!` `macro_rules!` macro, exported crate-internally via `pub(crate) use`.
- **Modify `src/lib.rs`** — declare `mod macros;` and `#[cfg(feature = "unknown_variants")] mod unknown;`.
- **Modify the six enum modules** — rewrite each enum as an `unknown_fallback!` invocation, drop the now-redundant `use serde::{…}` import, add `#[derive(Ord, PartialOrd)]` where the enum had it, and switch `Display` `Unknown` arms to `*s`:
  - `src/set/set_type.rs` (snake_case, has `Display`, per-variant `rename = "box"`)
  - `src/card/frame_effect.rs` (lowercase, has `Display`)
  - `src/card/layout.rs` (snake_case)
  - `src/card/finishes.rs` (lowercase, derives `Ord`)
  - `src/card/security_stamp.rs` (lowercase, derives `Ord`, `allow(missing_docs)`)
  - `src/card/promo_types.rs` (lowercase, derives `Ord`, `allow(missing_docs)`)

---

## Task 1: Vertical slice — interner, macro, and `SetType` retrofit

Proves the whole mechanism in the real crate end-to-end.

**Files:**
- Create: `src/unknown.rs`
- Create: `src/macros.rs`
- Modify: `src/lib.rs:70-80` (module declarations)
- Modify: `src/set/set_type.rs` (full enum + `Display`)

- [ ] **Step 1: Create `src/macros.rs`**

```rust
//! Crate-internal macros.

/// Declares a serde enum that gains an `Unknown` fallback under the
/// `unknown_variants` / `unknown_variants_slim` features and is
/// `#[non_exhaustive]` otherwise.
///
/// Under `unknown_variants` the fallback is `Unknown(&'static str)`, interned
/// (and therefore `Copy`) by [`crate::unknown`]. Under `unknown_variants_slim`
/// it is a unit `Unknown` via `#[serde(other)]`. With neither feature the enum
/// has no `Unknown` arm and stays `#[non_exhaustive]`.
///
/// Callers pass any extra container attributes (`#[serde(rename_all = …)]`,
/// `#[derive(Ord, PartialOrd)]`, `#[allow(missing_docs)]`, doc comments) and
/// per-variant attributes through verbatim. All target enums are unit-variant.
///
/// See `docs/superpowers/specs/2026-06-02-unknown-enum-fallback-design.md`.
macro_rules! unknown_fallback {
    (
        $(#[$enum_meta:meta])*
        $vis:vis enum $name:ident {
            $(
                $(#[$var_meta:meta])*
                $var:ident
            ),* $(,)?
        }
    ) => {
        #[derive(::serde::Serialize, ::serde::Deserialize, Clone, Copy, Eq, PartialEq, Hash, Debug)]
        #[cfg_attr(
            all(not(feature = "unknown_variants"), not(feature = "unknown_variants_slim")),
            non_exhaustive
        )]
        #[cfg_attr(test, serde(deny_unknown_fields))]
        $(#[$enum_meta])*
        $vis enum $name {
            $(
                $(#[$var_meta])*
                $var,
            )*

            #[cfg_attr(
                docsrs,
                doc(cfg(any(feature = "unknown_variants", feature = "unknown_variants_slim")))
            )]
            #[cfg(feature = "unknown_variants")]
            #[serde(untagged)]
            /// An unknown value not yet modeled by this version of the crate.
            Unknown(#[serde(with = "crate::unknown::interned")] &'static str),

            #[cfg_attr(
                docsrs,
                doc(cfg(any(feature = "unknown_variants", feature = "unknown_variants_slim")))
            )]
            #[cfg(all(not(feature = "unknown_variants"), feature = "unknown_variants_slim"))]
            #[serde(other)]
            /// An unknown value not yet modeled by this version of the crate.
            Unknown,
        }
    };
}

pub(crate) use unknown_fallback;
```

- [ ] **Step 2: Create `src/unknown.rs`**

```rust
//! Interning support for `Unknown(&'static str)` enum fallbacks.
//!
//! Compiled only under the `unknown_variants` feature. A new value Scryfall
//! sends that this crate version has no typed variant for is interned here into
//! a `&'static str`, so the enum's `Unknown` arm stays `Copy`. Distinct values
//! are leaked once and deduped; the unknown vocabulary is tiny in practice. See
//! `docs/superpowers/specs/2026-06-02-unknown-enum-fallback-design.md`.

use std::borrow::Cow;
use std::collections::HashSet;
use std::sync::Mutex;

use once_cell::sync::Lazy;
use serde::{Deserialize, Deserializer, Serializer};

static POOL: Lazy<Mutex<HashSet<&'static str>>> = Lazy::new(|| Mutex::new(HashSet::new()));

/// Interns `s` to a `&'static str`. Equal strings return the same pointer;
/// each distinct string is leaked exactly once.
fn intern(s: &str) -> &'static str {
    let mut pool = POOL.lock().unwrap();
    if let Some(&hit) = pool.get(s) {
        return hit;
    }
    let leaked: &'static str = Box::leak(Box::from(s));
    pool.insert(leaked);
    leaked
}

/// `#[serde(with = "crate::unknown::interned")]` codec for an interned
/// `&'static str` field.
pub(crate) mod interned {
    use super::*;

    pub fn serialize<S: Serializer>(s: &&'static str, ser: S) -> Result<S::Ok, S::Error> {
        ser.serialize_str(s)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<&'static str, D::Error> {
        let s = Cow::<str>::deserialize(d)?;
        Ok(intern(&s))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn intern_dedups_to_same_pointer() {
        assert!(std::ptr::eq(intern("alpha"), intern("alpha")));
        assert!(!std::ptr::eq(intern("alpha"), intern("beta")));
    }

    #[test]
    fn codec_round_trips_through_set_type() {
        use crate::set::SetType;
        let v: SetType = serde_json::from_str(r#""totally-new-set-type""#).unwrap();
        assert_eq!(v, SetType::Unknown("totally-new-set-type"));
        assert_eq!(serde_json::to_string(&v).unwrap(), r#""totally-new-set-type""#);
    }
}
```

- [ ] **Step 3: Wire both modules into `src/lib.rs`**

At the **top** of the module-declaration block (immediately before `pub mod bulk;`, currently `src/lib.rs:70`), add:

```rust
mod macros;
#[cfg(feature = "unknown_variants")]
mod unknown;
pub mod bulk;
```

(That is: insert `mod macros;` and the feature-gated `mod unknown;` just before the existing `pub mod bulk;`. Both are private; the macro is reached by path via `crate::macros::unknown_fallback`, which is declaration-order independent. Leave the existing `mod util;` line untouched.)

- [ ] **Step 4: Run — interner tests fail to compile (red)**

Run: `cargo test --features unknown_variants --lib unknown::`
Expected: FAILS — `codec_round_trips_through_set_type` references `SetType::Unknown("…")` (a `&'static str`), but `SetType` still has `Unknown(Box<str>)`. Compile error: mismatched types / expected `Box<str>`.

- [ ] **Step 5: Retrofit `src/set/set_type.rs`**

Replace the import (`src/set/set_type.rs:4`):

```rust
use serde::{Deserialize, Serialize};
```

with:

```rust
use crate::macros::unknown_fallback;
```

Replace the enum header — the `#[derive(...)]`/`#[cfg_attr(...)]`/`#[serde(...)]` block and `pub enum SetType {` (currently `src/set/set_type.rs:8-20`) — i.e. these lines:

```rust
#[derive(Serialize, Deserialize, Clone, Eq, PartialEq, Hash, Debug)]
#[cfg_attr(not(feature = "unknown_variants"), derive(Copy))]
#[cfg_attr(
    all(
        not(feature = "unknown_variants"),
        not(feature = "unknown_variants_slim")
    ),
    non_exhaustive
)]
#[cfg_attr(test, serde(deny_unknown_fields))]
#[serde(rename_all = "snake_case")]
pub enum SetType {
```

with (note: the enum doc comment on `src/set/set_type.rs:6-7` stays directly above this, now inside the macro):

```rust
unknown_fallback! {
#[serde(rename_all = "snake_case")]
pub enum SetType {
```

Then delete the two trailing `Unknown` arms (currently the block from `#[cfg_attr(docsrs, …)]` through `Unknown,` just before the enum's closing brace) so the variant list ends at `Minigame,`. Leave the variant list otherwise untouched (no reindenting). After the enum's closing `}`, add one more `}` to close the macro invocation. The result tail looks like:

```rust
    /// Mini game sets
    Minigame,
}
}
```

Finally, in the `Display` impl, change the `Unknown` arm (currently `src/set/set_type.rs`):

```rust
                #[cfg(feature = "unknown_variants")]
                SetType::Unknown(s) => s,
```

to:

```rust
                #[cfg(feature = "unknown_variants")]
                SetType::Unknown(s) => *s,
```

- [ ] **Step 6: Run — interner tests pass (green)**

Run: `cargo test --features unknown_variants --lib unknown::`
Expected: PASS (`intern_dedups_to_same_pointer`, `codec_round_trips_through_set_type`).

- [ ] **Step 7: Verify all three feature modes build and the existing harness passes**

Run each; all must succeed:

```bash
cargo build
cargo build --features unknown_variants
cargo build --features unknown_variants_slim
cargo test --features unknown_variants --test variants-feature
cargo test --features unknown_variants_slim --test variants-feature
cargo test --test variants-feature
```

Expected: all compile; the `variants-feature` `deserialize` test (which includes `SetType::Unknown("foo".into())`) passes under `unknown_variants`, and the slim/default mods compile and pass.

- [ ] **Step 8: Commit**

```bash
git add src/unknown.rs src/macros.rs src/lib.rs src/set/set_type.rs
git commit -m "feat: interned &'static str Unknown repr via unknown_fallback! (SetType)

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

## Task 2: Retrofit `FrameEffect`

**Files:**
- Modify: `src/card/frame_effect.rs` (enum + `Display`)

- [ ] **Step 1: Swap the import**

Replace `src/card/frame_effect.rs:1`:

```rust
use serde::{Deserialize, Serialize};
```

with:

```rust
use crate::macros::unknown_fallback;
```

- [ ] **Step 2: Wrap the enum in the macro**

Replace the header block + `pub enum FrameEffect {` (currently `src/card/frame_effect.rs:8-19`):

```rust
#[derive(Serialize, Deserialize, Clone, Eq, PartialEq, Hash, Debug)]
#[cfg_attr(not(feature = "unknown_variants"), derive(Copy))]
#[cfg_attr(
    all(
        not(feature = "unknown_variants"),
        not(feature = "unknown_variants_slim")
    ),
    non_exhaustive
)]
#[cfg_attr(test, serde(deny_unknown_fields))]
#[serde(rename_all = "lowercase")]
pub enum FrameEffect {
```

with (the enum doc comment on lines 3-7 stays directly above, now inside the macro):

```rust
unknown_fallback! {
#[serde(rename_all = "lowercase")]
pub enum FrameEffect {
```

Delete the two trailing `Unknown` arms (the `#[cfg_attr(docsrs, …)]` … `Unknown(Box<str>)` and `… Unknown,` blocks) so the variant list ends at `PlaceholderImage,`. After the enum's closing `}`, add a second `}` to close the macro:

```rust
    /// Placeholder Image
    PlaceholderImage,
}
}
```

- [ ] **Step 3: Fix the `Display` arm**

Replace (currently `src/card/frame_effect.rs:157-158`):

```rust
                #[cfg(feature = "unknown_variants")]
                Unknown(s) => s,
```

with:

```rust
                #[cfg(feature = "unknown_variants")]
                Unknown(s) => *s,
```

- [ ] **Step 4: Verify three modes**

```bash
cargo build
cargo build --features unknown_variants
cargo build --features unknown_variants_slim
cargo test --features unknown_variants --test variants-feature
```

Expected: all pass (the `variants-feature` `deserialize` test covers `FrameEffect::Unknown("foo".into())`).

- [ ] **Step 5: Commit**

```bash
git add src/card/frame_effect.rs
git commit -m "feat: retrofit FrameEffect to unknown_fallback!

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

## Task 3: Retrofit `Layout`

**Files:**
- Modify: `src/card/layout.rs` (no `Display` impl exists)

- [ ] **Step 1: Swap the import**

Replace `src/card/layout.rs:1`:

```rust
use serde::{Deserialize, Serialize};
```

with:

```rust
use crate::macros::unknown_fallback;
```

- [ ] **Step 2: Wrap the enum**

Replace the header block + `pub enum Layout {` (currently `src/card/layout.rs:16-27`):

```rust
#[derive(Serialize, Deserialize, Clone, Eq, PartialEq, Hash, Debug)]
#[cfg_attr(not(feature = "unknown_variants"), derive(Copy))]
#[cfg_attr(
    all(
        not(feature = "unknown_variants"),
        not(feature = "unknown_variants_slim")
    ),
    non_exhaustive
)]
#[cfg_attr(test, serde(deny_unknown_fields))]
#[serde(rename_all = "snake_case")]
pub enum Layout {
```

with (the multi-line enum doc comment on lines 3-15 stays directly above, now inside the macro):

```rust
unknown_fallback! {
#[serde(rename_all = "snake_case")]
pub enum Layout {
```

Delete the two trailing `Unknown` arms so the list ends at `Case,`, then add a second `}` after the enum's closing brace:

```rust
    /// Case
    Case,
}
}
```

- [ ] **Step 3: Verify three modes**

```bash
cargo build
cargo build --features unknown_variants
cargo build --features unknown_variants_slim
cargo test --features unknown_variants --test variants-feature
```

Expected: all pass (`Layout::Unknown("foo".into())` covered).

- [ ] **Step 4: Commit**

```bash
git add src/card/layout.rs
git commit -m "feat: retrofit Layout to unknown_fallback!

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

## Task 4: Retrofit `Finishes` (derives `Ord`)

**Files:**
- Modify: `src/card/finishes.rs`

- [ ] **Step 1: Swap the import**

Replace `src/card/finishes.rs:1`:

```rust
use serde::{Deserialize, Serialize};
```

with:

```rust
use crate::macros::unknown_fallback;
```

- [ ] **Step 2: Wrap the enum, preserving `Ord`/`PartialOrd`**

Replace the header block + `pub enum Finishes {` (currently `src/card/finishes.rs:4-15`):

```rust
#[derive(Serialize, Deserialize, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
#[cfg_attr(test, serde(deny_unknown_fields))]
#[cfg_attr(not(feature = "unknown_variants"), derive(Copy))]
#[cfg_attr(
    all(
        not(feature = "unknown_variants"),
        not(feature = "unknown_variants_slim")
    ),
    non_exhaustive
)]
#[serde(rename_all = "lowercase")]
pub enum Finishes {
```

with (the `/// The finish the card can come in.` doc comment on line 3 stays directly above, now inside the macro; `Ord, PartialOrd` are reintroduced as a caller-supplied derive because the macro's base derive omits them):

```rust
unknown_fallback! {
#[derive(Ord, PartialOrd)]
#[serde(rename_all = "lowercase")]
pub enum Finishes {
```

Delete the two trailing `Unknown` arms so the list ends at `Etched,`, then add a second `}`:

```rust
    /// Etched foil.
    Etched,
}
}
```

- [ ] **Step 3: Verify three modes**

```bash
cargo build
cargo build --features unknown_variants
cargo build --features unknown_variants_slim
cargo test --features unknown_variants --test variants-feature
```

Expected: all pass.

- [ ] **Step 4: Commit**

```bash
git add src/card/finishes.rs
git commit -m "feat: retrofit Finishes to unknown_fallback!

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

## Task 5: Retrofit `SecurityStamp` (derives `Ord`, `allow(missing_docs)`)

**Files:**
- Modify: `src/card/security_stamp.rs`

- [ ] **Step 1: Swap the import**

Replace `src/card/security_stamp.rs:1`:

```rust
use serde::{Deserialize, Serialize};
```

with:

```rust
use crate::macros::unknown_fallback;
```

- [ ] **Step 2: Wrap the enum, preserving `Ord`/`PartialOrd` and `allow(missing_docs)`**

Replace the header block + `pub enum SecurityStamp {` (currently `src/card/security_stamp.rs:4-16`):

```rust
#[derive(Serialize, Deserialize, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
#[cfg_attr(test, serde(deny_unknown_fields))]
#[cfg_attr(
    all(
        not(feature = "unknown_variants"),
        not(feature = "unknown_variants_slim")
    ),
    non_exhaustive
)]
#[cfg_attr(not(feature = "unknown_variants"), derive(Copy))]
#[serde(rename_all = "lowercase")]
#[allow(missing_docs)]
pub enum SecurityStamp {
```

with (the `/// The security stamp on this card, if any.` doc comment on line 3 stays directly above, now inside the macro):

```rust
unknown_fallback! {
#[derive(Ord, PartialOrd)]
#[serde(rename_all = "lowercase")]
#[allow(missing_docs)]
pub enum SecurityStamp {
```

Delete the two trailing `Unknown` arms so the list ends at `Heart,`, then add a second `}`:

```rust
    Heart,
}
}
```

- [ ] **Step 3: Verify three modes**

```bash
cargo build
cargo build --features unknown_variants
cargo build --features unknown_variants_slim
cargo test --features unknown_variants --test variants-feature
```

Expected: all pass.

- [ ] **Step 4: Commit**

```bash
git add src/card/security_stamp.rs
git commit -m "feat: retrofit SecurityStamp to unknown_fallback!

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

## Task 6: Retrofit `PromoType` (derives `Ord`, `allow(missing_docs)`, ~120 variants)

**Files:**
- Modify: `src/card/promo_types.rs`

- [ ] **Step 1: Swap the import**

Replace `src/card/promo_types.rs:1`:

```rust
use serde::{Deserialize, Serialize};
```

with:

```rust
use crate::macros::unknown_fallback;
```

- [ ] **Step 2: Wrap the enum**

Replace the header block + `pub enum PromoType {` (currently `src/card/promo_types.rs:4-17`):

```rust
#[derive(Serialize, Deserialize, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
#[cfg_attr(not(feature = "unknown_variants"), derive(Copy))]
#[cfg_attr(
    all(
        not(feature = "unknown_variants"),
        not(feature = "unknown_variants_slim")
    ),
    non_exhaustive
)]
#[cfg_attr(test, serde(deny_unknown_fields))]
#[serde(rename_all = "lowercase")]
#[allow(missing_docs)]
pub enum PromoType {
```

with (the `/// The finish the card can come in.` doc comment on line 3 stays directly above, now inside the macro):

```rust
unknown_fallback! {
#[derive(Ord, PartialOrd)]
#[serde(rename_all = "lowercase")]
#[allow(missing_docs)]
pub enum PromoType {
```

Leave the ~120 variants (`Alchemy` … `Wizardsplaynetwork`) exactly as they are. Delete the two trailing `Unknown` arms so the list ends at `Wizardsplaynetwork,`, then add a second `}` to close the macro:

```rust
    Wizardsplaynetwork,
}
}
```

- [ ] **Step 3: Verify three modes**

```bash
cargo build
cargo build --features unknown_variants
cargo build --features unknown_variants_slim
cargo test --features unknown_variants --test variants-feature
```

Expected: all pass (the `variants-feature` `match_on_promo_type` exhaustiveness fn and `PromoType::Unknown("foo".into())` are covered).

- [ ] **Step 4: Commit**

```bash
git add src/card/promo_types.rs
git commit -m "feat: retrofit PromoType to unknown_fallback!

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

## Task 7: Lock `Copy` and verify the full matrix

The macro derives `Copy` unconditionally, so the compiler already enforces it; these static-assertions guard against a future edit silently dropping it, and the round-trip/order tests document behavior.

**Files:**
- Modify: `src/unknown.rs` (extend the `#[cfg(test)] mod tests`)

- [ ] **Step 1: Add `Copy` locks and an ordering test**

In `src/unknown.rs`, inside `mod tests`, add after the existing `use super::*;`:

```rust
    use static_assertions::assert_impl_all;

    use crate::card::{Finishes, FrameEffect, Layout, PromoType, SecurityStamp};
    use crate::set::SetType;

    assert_impl_all!(SetType: Copy);
    assert_impl_all!(FrameEffect: Copy);
    assert_impl_all!(Layout: Copy);
    assert_impl_all!(Finishes: Copy);
    assert_impl_all!(SecurityStamp: Copy);
    assert_impl_all!(PromoType: Copy);

    #[test]
    fn unknown_orders_by_content() {
        // Finishes derives Ord; Unknown must compare lexicographically, not by
        // interned pointer identity.
        assert!(Finishes::Unknown("aaa") < Finishes::Unknown("bbb"));
    }
```

- [ ] **Step 2: Run the unknown-module tests**

Run: `cargo test --features unknown_variants --lib unknown::`
Expected: PASS — `intern_dedups_to_same_pointer`, `codec_round_trips_through_set_type`, `unknown_orders_by_content`, and the crate compiles with all six `assert_impl_all!(_: Copy)` (proving `Copy` was restored).

- [ ] **Step 3: Full feature-matrix verification**

Run each; all must succeed:

```bash
cargo build
cargo build --features unknown_variants
cargo build --features unknown_variants_slim
cargo test --test variants-feature
cargo test --features unknown_variants --test variants-feature
cargo test --features unknown_variants_slim --test variants-feature
cargo test --features unknown_variants --lib
```

Expected: every command compiles and passes. (Ignored network tests stay ignored; do not run `--ignored`.)

- [ ] **Step 4: Confirm no stray `Box<str>` / old cfg remain in the six enums**

Run: `grep -rn "Unknown(Box<str>)\|derive(Copy)" src/card/promo_types.rs src/set/set_type.rs src/card/frame_effect.rs src/card/layout.rs src/card/finishes.rs src/card/security_stamp.rs`
Expected: no matches (the macro now owns `Copy` and the `Unknown` arm).

- [ ] **Step 5: Commit**

```bash
git add src/unknown.rs
git commit -m "test: lock Copy for the six netted enums and Unknown ordering

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

## Self-Review

**Spec coverage:**
- `Box<str>` → interned `&'static str` repr — Tasks 1–6 (per enum).
- Hand-rolled `Lazy` interner + `interned` codec — Task 1 (`src/unknown.rs`).
- `unknown_fallback!` macro — Task 1 (`src/macros.rs`).
- `Copy` unconditional — macro base derive (Task 1); locked in Task 7.
- `Display` arms `=> *s` — Tasks 1 (`SetType`) and 2 (`FrameEffect`); the other four have no `Display`.
- `Ord`/`PartialOrd` preserved where present — Tasks 4, 5, 6 add `#[derive(Ord, PartialOrd)]`.
- Default behavior unchanged (`non_exhaustive`, hard-fail) — macro emits it; verified by `cargo test --test variants-feature` in default mode.
- Per-repr tests roll in — interner tests + `Copy` locks live in `src/unknown.rs`.
- Out of scope (Branch 2/3) — no new enums; no edits to `tests/variants-feature/*`, CI, or `scripts/`.

**Placeholder scan:** none — every step carries exact code, paths, commands, and expected results.

**Type/name consistency:** `unknown_fallback!`, `crate::macros::unknown_fallback`, `crate::unknown::interned`, `intern`, and `Unknown(&'static str)` are used identically across all tasks. `#[derive(Ord, PartialOrd)]` is added in exactly the three tasks whose enums had `Ord` (Finishes, SecurityStamp, PromoType) and omitted for the three that didn't (SetType, FrameEffect, Layout), matching the current source.

**Verification provenance:** the macro, the interner, the `serde(untagged)` + `serde(with)` combination across all three feature modes, and the `pub(crate) use` macro path were each compiled and run in throwaway crates before this plan was written.
