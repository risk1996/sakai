# Cgroup v2 value and file inconsistencies

Address duplicate-key handling first. It can silently hide malformed input
in every parser that uses the shared keyed collector. The other findings can
be handled as targeted codec fixes or API cleanup.

## Snapshot encoding and parsing

- **Map keys can produce invalid text.** Adding a resource named `two words` to a
  `MiscCurrent` snapshot produces `two words 0` when encoded. The same parser
  rejects that line because it expects one key token and one value token.
  The shared map encoder emits keys verbatim, while keyed write commands check
  that the key is a single token.
  **Medium priority.** A caller can save a modified snapshot successfully and
  discover the invalid key only when loading it again.
  [Map codec](../sakai/src/parse/codec.rs#L65) · [Write key validation](../sakai/src/cgroup/common/interface.rs#L121)
- **Memory snapshots can encode without required fields.** Both
  `MemoryStat::default()` and `MemoryNumaStat::default()` encode successfully
  as an empty string. Both parsers require `anon` and `file`. Editing the
  public maps can also remove these fields from a previously valid snapshot.
  **Medium priority.** Default and edited snapshots are easy to construct with
  the public API, so callers can persist incomplete data without an error.
  [Memory stat](../sakai/src/cgroup/v2/memory/stat.rs#L56) · [NUMA stat](../sakai/src/cgroup/v2/memory/numa_stat.rs#L104)
- **Distinct controller keys can collapse on a round trip.** A
  `CgroupStat` subsystem map can contain both `CgroupController::Cpu` and
  `CgroupController::Other("cpu".into())`. The encoder emits two
  `nr_subsys_cpu` rows. Parsing the result yields one entry.
  **Medium priority.** Round-tripping can silently discard a counter. The
  collision requires a caller to construct a noncanonical controller key.
  [Subsystem encoder](../sakai/src/cgroup/v2/core/state.rs#L129)

## Parser entry points

- **`cgroup.type` handles whitespace differently across entry points.**
  `CgroupType::decode("domain\n")` succeeds because the file decoder trims
  input. `"domain\n".parse::<CgroupType>()` fails.
  **Medium priority.** Offline callers must trim ordinary saved kernel output
  before parsing it directly. Live reads already handle the newline.
  [File decoder](../sakai/src/cgroup/v2/core.rs#L88)
- **Duplicate keys can hide malformed values in some files.**
  `pids.events` accepts `"max bad\nmax 0"` because the shared keyed parser
  collects rows before parsing values. `memory.stat` rejects
  `"anon bad\nanon 0\nfile 0"` because it parses values as it reads rows.
  **High priority.** Malformed input can appear valid and produce a plausible
  counter. The shared collector gives this behavior a wider reach than the
  `pids.events` example.
  [Keyed parser](../sakai/src/parse/mod.rs#L23) · [Memory stat parser](../sakai/src/cgroup/v2/memory/stat.rs#L76)

## File type conventions

- **Write command types vary across interfaces.** `MemoryMax` takes the
  inner `MaxOr<Bytes>`, `CpuMax` takes a borrowed snapshot, and `CgroupType`
  takes `()` to request `threaded`. A borrowed snapshot can also be
  projected to a command. For example, `CpusetPartition` encodes the snapshot
  `root invalid (test reason)` as `root` for a write. Those signatures make
  different promises about what the caller supplies and what gets written.
  **Low priority for general consistency.** The scalar, structured value,
  and topology action have different semantics that justify different inputs.
  **Medium priority for `CpusetPartition`.** Passing an invalid snapshot to
  the writer can issue a valid state-change request, so the accepted inputs
  and their effect should be explicit.
  [Scalar commands](../sakai/src/scalar.rs#L111) · [Default command macro](../sakai/src/cgroup/common/interface.rs#L113) · [Topology command](../sakai/src/cgroup/v2/core.rs#L92) · [Partition command](../sakai/src/cgroup/v2/cpuset/partition.rs#L43)
- **Local event files expose different snapshot shapes.** `PidsEventsLocal`
  is a file marker that returns `PidsEvents`. `MemoryEventsLocal` is its own
  snapshot type and shares `MemoryEventCounts` with `MemoryEvents`.
  **Low priority.** The difference affects type signatures and code reuse.
  Aligning the shapes would change public return types, so it can wait for
  a planned breaking API revision.
  [PID events](../sakai/src/cgroup/v2/pids.rs#L62) · [Memory events](../sakai/src/cgroup/v2/memory/events.rs#L50)
