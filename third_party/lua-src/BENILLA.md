# benilla's `lua-src` fork: what differs from upstream, and how to check

Upstream: [`mlua-rs/lua-src-rs`](https://github.com/mlua-rs/lua-src-rs), MIT, version `550.0.0`,
the version mlua 0.11 resolves to. Wired in through `[patch.crates-io]` in the workspace root.

## Why a fork exists at all

By default, benilla's Lua has to accept exactly the grammar the 1.12.1 client's Lua accepts, no more
and no less. Every compatibility hunk below follows that rule: three restore 5.0 constructs 5.1
deleted, three disable 5.1 constructs 5.0 never had. A grammar difference in either direction is
invisible to every instrument until an addon trips over it, and the addon corpus asks about the
difference directly. The explicit Wrath switch documented below restores stock 5.1 behavior.

The 1.12 addon corpus is Lua 5.0 code. It uses the iterator-less generic-for:

```lua
for k, v in someTable do ... end        -- no pairs(), no iterator function
```

183 of 218 corpus addons are reached by it (118 carry it, 65 inherit it through a declared
dependency), across 1,163 sites. On stock Lua 5.1 it raises `attempt to call a table value`, the
first session-start error for 60 of the 218.

Lua 5.1 removed it at the opcode level, which is why no layer above the VM can reach it: a `__call`
metamethod would need a per-type default metatable for tables, which 5.1 does not have; rewriting
the chunk source means parsing Lua with a regex; and `lua-src` ships 5.1 through 5.5, every one of
which removed it.

## What is not being done

benilla is not adopting Lua 5.0. It stays on 5.1.5 and restores the behaviour of a single opcode
that Lua itself shipped and labelled `/* for compatibility only */`. Every other 5.1 fix, and the
whole of mlua's surface, stay exactly as upstream.

## Runtime dialect seam

Vanilla remains the default. To select stock Lua 5.1 behavior for a Wrath Lua
state, set the boolean registry entry `benilla.wrath51` to `true` before
installing or loading any scripts. The engine can do this through mlua's named
registry API; the vendored parser, lexer, and VM read the registry table
directly with `luaH_getstr` and `luaS_newliteral`, without making Lua stack
calls. The setting belongs to the Lua registry (shared by coroutines belonging
to that state), not to process globals or the Lua global environment, so
independent Vanilla and Wrath states can coexist.

Only boolean `true` selects Wrath. A missing key, `false`, or a value of any
other type preserves Vanilla behavior.

The switch gates all dialect-specific executable changes described below:

* Wrath restores 5.1's `...` expression, `#`, and `%` parser arms.
* Wrath uses 5.1's constructor semicolon handling and record-field table-size
  hint; Vanilla keeps the two 5.0 constructor behaviors.
* Wrath leaves a table-valued generic-for generator to 5.1's normal call path;
  Vanilla retains the 5.0 table-to-global-`next` substitution.
* Wrath restores 5.1's nested `[[...]]` advisory error and non-nesting long
  string handling; Vanilla retains 5.0 nesting.
* Wrath limits each native `string.find`, `string.match`, `gmatch` iterator
  call, or `string.gsub` call to 100,000,000 deterministic matcher steps and 200
  recursive matcher frames. Plain-search byte comparisons, search-position iterations, and recursive/
  backtrack matcher entries consume the same per-call budget. Exhaustion raises `pattern
  matching work limit exceeded` or `pattern matching recursion limit exceeded`.
  Vanilla deliberately retains the uncapped upstream matcher.

The matcher limit counts native work rather than elapsed time. It bounds the
pathological backtracking/search CPU that runs below Lua's VM hook, but it is
not wall-clock enforcement or full native-call preemption. Other native string
operations remain governed by Lua's allocator; in particular `string.rep` is
not part of this matcher cap and the embedding's 128 MiB allocator limit still
handles its output growth.

`LUA_QL` remains a compile-time macro and therefore keeps the Vanilla 5.0
backtick/apostrophe spelling in both dialects. Making every exception quote
state-dependent would require replacing a broad, upstream-wide compile-time
format-string facility and is intentionally outside this bounded seam.

## The delta, and the command that proves it

The original eight compatibility hunks are now runtime-gated, with the shared
registry reader in `lua-5.1.5/ldialect.h` and the long-string gate in `llex.c`.
`src/lib.rs` additionally differs by having Lua 5.2/5.3/5.4/5.5 stripped
from the `Version` enum (with their source trees deleted): benilla builds 5.1 and only 5.1, and a
fork that still offered the other four would answer a request for one with a missing directory at
build time instead of a compile error.

Five hunks restore what 5.1 changed or removed:

| file | hunk | why |
|---|---|---|
| `lvm.c` | 5.0's `OP_TFORPREP` table-to-`next` substitution, folded into the top of `OP_TFORLOOP` | `for k, v in someTable do` raised "attempt to call a table value" |
| `luaconf.h` | `LUA_COMPAT_LSTR` `1` to `2` | 5.1 kept 5.0's `[[...]]` nesting machinery and put an advisory error in front of it; two corpus addons died on "nesting of `[[...]]` is deprecated" |
| `luaconf.h` | `LUA_QL(x)` back to 5.0's `` `x' `` from 5.1's `'x'` | every Lua error message quotes a program element, and `WoW.exe`'s own `.rdata` carries all five formats in the 5.0 spelling |
| `lparser.c` | 5.0's compat-semicolon skip restored at the top of `constructor()`'s field loop, 5.0's own line verbatim | one extra `;` after a field separator inside a table constructor (`Back_Title = AL["Factions"];;`, which AtlasLoot writes) is accepted by the client and was a parse error here |
| `lparser.c` | `recfield`'s `cc->nh++` moved back inside its `TK_NAME` arm, 5.0's own placement | a `[expr] = value` constructor field credits neither `OP_NEWTABLE` size hint in 5.0, so the table is born on the dummy node |

Three delete what 5.1 added, all in `lparser.c`, all byte-read out of the client's own parser
(`simpleexp 0x6fd240`, `getunopr 0x6fe0a0`, `getbinopr 0x6fe0c0`), all landing on 5.0's own
`unexpected symbol` at `prefixexp 0x6fde40`:

| hunk | why |
|---|---|
| `simpleexp`'s `case TK_DOTS` deleted: `...` is not an expression | the Ace2 corpus asks which interpreter it is on by compiling `return function(...) return ... end`; 170 `loadstring` sites in the corpus are that question, and 92 library files in 24 folders branch on the answer |
| `getunopr`'s `case '#'` deleted: no length operator | the client's `getunopr` tests exactly two tokens (`-`, `not`) and its `OPR_NOUNOPR` is 2, a three-member enum; 5.0 asks a table with `table.getn` and a string with `string.len` |
| `getbinopr`'s `case '%'` deleted: no modulo operator | the client's `getbinopr` switch is based at `'*'` (0x2A), so `%` (0x25) is below its range and reaches `OPR_NOBINOPR` = 14, a fifteen-member `BinOpr`; 5.0 spells it `math.mod` |

The deletions are safe because nothing the reference runs uses those constructs: a comment- and
string-stripping scan of the 1.12 FrameXML (177 files), GlueXML, Blizzard's own addons and the
corpus finds zero sites of all three. No chunk of benilla's own Lua may use a construct the client's
parser rejects.

To verify the Lua sources against upstream at any time:

```sh
# 550.0.0 is the pinned version; adjust the path if cargo's registry hash differs.
diff -r third_party/lua-src/lua-5.1.5 \
  ~/.cargo/registry/src/*/lua-src-550.0.0/lua-5.1.5
```

That diff includes the eight hunks in the tables above plus their runtime
dialect gates, `ldialect.h`, and the `llex.c` long-string gate.

## The generic-for, in detail

The `lvm.c` comment carries the three details that are the client's own, each byte-read there: the
substitution test is a bare type-tag equality that never consults a metatable (a table carrying
`__call` still gets `next`); the callee is the global `next`, read raw and fetched fresh at every loop
entry (an addon assigning `next = myfn` changes every later generic-for in the session); and userdata
is not substituted. The substitution happens in `OP_TFORPREP`, an opcode 5.1 deleted, so the top of
`OP_TFORLOOP` is where 5.1 can host it.
