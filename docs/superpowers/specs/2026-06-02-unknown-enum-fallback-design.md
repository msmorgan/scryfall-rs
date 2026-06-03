# Unknown-enum-value fallback — design (Phase 1)

- **Date:** 2026-06-02
- **Status:** Approved pending spec review
- **Scope:** Phase 1 only. Catalog-driven codegen is a separate, later spec.

## Problem

scryfall-rs models many Scryfall string-valued fields as Rust enums with
hardcoded variants. When Scryfall introduces a new value (a new set type, promo
type, frame effect, …), deserialization **hard-fails** for downstream users
until the crate ships a new variant — the "constant deserialization failures"
that appear whenever new cards are printed.

An opt-in mitigation already exists. The `unknown_variants` /
`unknown_variants_slim` features add an `Unknown` fallback variant to a handful
of enums. Two gaps remain:

1. The `unknown_variants` fallback uses `Unknown(Box<str>)`, which forces the
   enum to drop `Copy`.
2. Only **6** enums carry the fallback; roughly a dozen more deserialized enums
   are still vulnerable.

## Goals

- Upgrade the `unknown_variants` fallback representation from `Box<str>` to an
  interned `&'static str`, restoring `Copy` while still capturing the string.
- Extend the fallback to the remaining vulnerable enums (with a conservative
  exclusion list).
- Eliminate the repeated `cfg` boilerplate with an `unknown_fallback!` macro.
- Keep the existing three-mode feature design and **default behavior unchanged**.

## Non-goals (Phase 1)

- Catalog-driven codegen (separate spec).
- Turning `String` / `Vec<String>` fields (`watermark`, `keywords`,
  `type_line`, `power`/`toughness`/`loyalty`) into enums.
- Changing default-build behavior. The default build stays strict (hard-fail on
  unknown), which is intentionally the **detector** signal.

## Key decisions (locked)

1. **Repr.** `unknown_variants` mode → `Unknown(&'static str)` (was
   `Box<str>`). Upgraded in place; breaking only for code that named the
   `Box<str>` payload type explicitly.
2. **Interner.** Hand-rolled, ~12 lines: a global
   `Lazy<Mutex<HashSet<&'static str>>>` (matching the crate's existing
   `once_cell::sync::Lazy` globals such as `ROOT_URL`), leak-once-per-distinct
   via `Box::leak`, deduped on lookup. No new dependency (`once_cell` is already
   a non-optional dep). `std::sync::LazyLock` is the std-only equivalent if
   preferred.
3. **Serde wiring.** The variant field uses
   `#[serde(with = "crate::unknown::interned")]` over a **bare `&'static str`** —
   no public newtype. Verified to compile and round-trip alongside per-variant
   `#[serde(untagged)]` (see "Validation").
4. **Copy.** Now **unconditional** across all three modes (`&'static str`, the
   slim unit, and the default unit variants are all `Copy`). Purely additive —
   `unknown_variants` users *gain* `Copy`.
5. **Coverage.** Net every deserialized enum with expansion history; skip
   tiny-stable sets; exclude `Color` structurally. (Lists below.)
6. **Memory model.** Accept the leak-per-distinct-unknown interner. It is
   bounded for real Scryfall data (the unknown vocabulary is tiny and deduped).
   It only grows without bound under adversarial JSON that streams unbounded
   *distinct* unknown strings into enum fields, where it degrades to a slow
   memory-growth DoS — acceptable for this crate. Bounding is deliberately not
   pursued: an evicting/bounded interner cannot hand out a `Copy` `&'static str`,
   so it would defeat the very properties we want. If that ever mattered, it
   would be taken as a breaking change *then*.

## Design

### Interner + serde codec — `src/unknown.rs`

Entirely gated behind `#[cfg(feature = "unknown_variants")]`, so default and
slim builds pull in no extra code and no global state.

```rust
use std::borrow::Cow;
use std::collections::HashSet;
use std::sync::Mutex;
use once_cell::sync::Lazy;
use serde::{Deserialize, Deserializer, Serializer};

static POOL: Lazy<Mutex<HashSet<&'static str>>> = Lazy::new(|| Mutex::new(HashSet::new()));

fn intern(s: &str) -> &'static str {
    let mut pool = POOL.lock().unwrap();
    if let Some(&hit) = pool.get(s) {
        return hit;
    }
    let leaked: &'static str = Box::leak(Box::from(s));
    pool.insert(leaked);
    leaked
}

/// `#[serde(with = "...")]` codec for an interned `&'static str` field.
pub(crate) mod interned {
    use super::*;

    pub fn serialize<S: Serializer>(s: &&'static str, ser: S) -> Result<S::Ok, S::Error> {
        ser.serialize_str(s)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<&'static str, D::Error> {
        let s = Cow::<str>::deserialize(d)?; // borrow if possible, else owned
        Ok(intern(&s))
    }
}
```

The only public-surface change is the variant payload itself: `Box<str>` →
`&'static str`. `intern` and the `interned` codec are crate-internal.

### `unknown_fallback!` macro

A `macro_rules!` that owns the whole cfg dance so each enum is declared plainly.
Call site:

```rust
unknown_fallback! {
    /// Scryfall's set categorization.
    #[derive(Default)]                 // optional extra derives pass through
    #[serde(rename_all = "snake_case")]
    pub enum SetType {
        Core, Expansion, Masters, /* ... */
        #[serde(rename = "box")] GiftBox,   // per-variant attrs pass through
    }
}
```

Expands to:

```rust
#[derive(Serialize, Deserialize, Clone, Copy, Eq, PartialEq, Hash, Debug)]
#[cfg_attr(
    all(not(feature = "unknown_variants"), not(feature = "unknown_variants_slim")),
    non_exhaustive
)]
#[cfg_attr(test, serde(deny_unknown_fields))]
/* …passed-through enum attrs (rename_all, extra derives)… */
pub enum SetType {
    /* …passed-through variants with their attrs… */

    #[cfg(feature = "unknown_variants")]
    #[serde(untagged)]
    Unknown(#[serde(with = "crate::unknown::interned")] &'static str),

    #[cfg(all(not(feature = "unknown_variants"), feature = "unknown_variants_slim"))]
    #[serde(other)]
    Unknown,
}
```

Notes:

- `Copy` is **unconditional**; the old
  `#[cfg_attr(not(feature = "unknown_variants"), derive(Copy))]` hack is removed.
- Grammar captures
  `$(#[$enum_meta:meta])* $vis:vis enum $name { $($(#[$var_meta:meta])* $var:ident),* $(,)? }`.
  Every target enum is unit-variant, so this covers them.
- `Display` / `FromStr` / inherent-method impls stay **outside** the macro,
  written normally (matching today's structure). The `Unknown` arm in a
  `Display` match becomes `Unknown(s) => *s`.

### Coverage

**Net it** — gets `Unknown(&'static str)` (full) / `Unknown` unit (slim):

- *Retrofit the existing 6 to `&'static str`:* `PromoType`, `SetType`,
  `FrameEffect`, `Layout`, `Finishes`, `SecurityStamp`.
- *Newly netted (all have expansion history):* `Rarity`, `Frame`,
  `BorderColor`, `Format` (re-added — now `Copy`-safe), `Languages`, `Game`,
  `Component`, `ProducedMana` (+ `UnfinityMana`).

**Skip** — tiny and never-expanded; these hard-fail in *all* modes:
`Legality`, `Source`, `ImageStatus`.

**Excluded (structural)** — `Color`. Its `#[repr(u8)]` bit-flag discriminants
(`White = 1 << 0`, …) drive the `Colors(u8)` bitset via `color as u8`; a
data-carrying variant is impossible, and MTG colors are a closed set anyway.

### Special cases

- **`Color`** — untouched.
- **`ProducedMana`** — already an untagged composite (`Color | UnfinityMana`).
  Add a hand-written untagged `Unknown(&'static str)` arm rather than going
  through the macro. `UnfinityMana` (a normal unit enum) goes through the macro.
- **`Format`** — its `Unknown` variant was removed in `2e7539a` (almost
  certainly to restore `Copy` for legality maps / search params). The
  `Copy`-preserving `&'static str` repr removes that reason, so the fallback is
  re-added.

### Feature-mode semantics (final)

| | `unknown_variants` | `unknown_variants_slim` | default (neither) |
|---|---|---|---|
| Netted enum gains | `Unknown(&'static str)` (untagged, interned) | `Unknown` unit (`#[serde(other)]`) | — |
| `Copy` | ✅ (was ❌ with `Box<str>`) | ✅ | ✅ |
| `non_exhaustive` | ❌ | ❌ | ✅ |
| Unknown value → | `Unknown("the-string")` | `Unknown` (string dropped) | **hard error** |

- **Default = strict = detector.** Netted enums stay `non_exhaustive` with no
  `Unknown` arm; an unknown value still hard-fails — the intentional CI signal
  to add a variant (and, later, the codegen trigger).
- Precedence unchanged: both features on → `unknown_variants` wins.
- Skipped enums hard-fail in all modes.

## Validation

A standalone snippet confirmed the serde mechanics before committing to the
bare-`&'static str` approach. With `#[derive(Serialize, Deserialize, Clone,
Copy, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]` on an enum whose `Unknown`
variant is `#[serde(untagged)] Unknown(#[serde(with = "interned")] &'static
str)`:

- known + renamed variants deserialize normally;
- unknown values fall into `Unknown`, interned;
- it round-trips back to the bare string (no variant tag);
- all derives hold, including `Copy`;
- the interner dedups to a single allocation (pointer equality);
- `Ord` on `Unknown` is by string content (deterministic across runs).

## Testing & CI

- **Detection model (extended, not changed).** `scheduled.yml`'s three weekly
  jobs keep deserializing bulk data via `cargo test all_cards -- --ignored`.
  After this change the **default** job hard-fails on any new netted value (the
  signal); the **`unknown_variants` / slim** jobs pass (graceful fallback
  proven).
- **Unit tests.** Extend `tests/variants-feature/{default_variants,
  unknown_variants, unknown_variants_slim}.rs`: feed an unknown string to
  representative netted enums and assert `Err` / `Unknown("…")` + `Display`
  round-trip / `Unknown` respectively. Add interner tests in `src/unknown.rs`
  (dedup pointer-eq, content-`Ord`, serde round-trip). Use `static_assertions`
  (already a dep) to lock `Copy` under every mode so it cannot regress.
- **Feature matrix.** `scripts/test-features.sh` already iterates every feature.

## Delivery plan (branching & sequencing)

- **Branch 1 — primary (repr).** Interner + `interned` codec + `unknown_fallback!`
  macro + retrofit the existing 6 enums to `&'static str`; `Copy` made
  unconditional; `Display` arms adjusted to `*s`. Per-repr unit tests roll in.
  No new enums, no CI-workflow edits. This is the headline objective.
- **Branch 2 — testing/CI infra (isolated).** Changes to the GitHub Actions
  workflows (`scheduled.yml`), `scripts/test-features.sh`, the variants-feature
  test harness, and the CI matrix — kept off the primary branch so the repr diff
  stays clean.
- **Branch 3 — exhaustiveness.** Extend the net to the newly-netted enums, with
  per-enum behavioral tests rolled in alongside their code.
  **Checkpoint: re-confirm the exact enum list with the maintainer before
  implementing.**

Interpretation in effect: "testing/CI changes" that get isolated = the CI
workflow + harness scripts (Branch 2); per-enum behavioral unit tests roll in
with the code they test (Branches 1 and 3).

## Phase 2 boundary

Catalog-driven codegen is a separate spec. The known-variant lists are what
codegen will later refresh; the `Unknown` net guarantees codegen lag can never
break users. Converting `String` fields into generated enums is out of scope
here.

## Open items

- Re-confirm the Branch-3 enum list before implementing (per maintainer
  request).
- Confirm the testing-split interpretation above.
