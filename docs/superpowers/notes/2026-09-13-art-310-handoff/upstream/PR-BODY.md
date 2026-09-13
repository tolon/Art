## What

`format_with_size` wrote two structures differently from pfs3aio's `format.c`:

1. **SUPERINDEX mode:** `superindex[0]` pointed at the anode index block instead of a super index
   (`SB`) block, while every reader walks `superindex → SB → IB → AB`: pfs3aio's `GetSuperBlock`,
   hst-imager's port, and this crate's own `AnodeReader`.
2. **Anodes 0–4** were left `(0, 0, 0)`. pfs3aio reserves them during format (`AllocAnode` leaves
   `clustersize 0, blocknr 0xffffffff, next 0`).

## Evidence

Measured with hst-imager 1.6.616 (a C# port of pfs3aio) on RDB images carrying pfs3aio 3.1 as the
`PDS\3` driver. At 1 GiB and 6 GiB, each fix was applied as its own variable:

| Format | 1 GiB | 6 GiB |
|---|---|---|
| unpatched | hst lists; hst's first `mkdir` fails `ERROR_DISK_FULL`; a later hst directory gets anode 1 | hst: `NullReferenceException`; libpfs3: `anode 5 not found` |
| SB level only | as unpatched | mounts; hst's first `mkdir` still fails |
| reserved anodes only | all ok | still unreadable |
| both (this PR) | all ok | all ok; the layout matches hst-imager's own format down to the root directory block |

The existing test suite passes unchanged; two tests are added in `tests/format.rs`.

## Not changed

The writer's `alloc_anode_block` still never allocates a second index block in small mode, or a second
super index block, where pfs3aio's `NewIndexBlock` / `NewSuperBlock` would. That is a separate change.
