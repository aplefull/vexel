#!/usr/bin/env python3

import csv
import argparse
from pathlib import Path
from dataclasses import dataclass


@dataclass
class Row:
    path: str
    decode_ms: float
    convert_ms: float
    width: int
    height: int
    mse: float | None
    error: str | None


def load_csv(path: str) -> dict[str, Row]:
    rows = {}
    with open(path, newline="") as f:
        reader = csv.DictReader(f)
        for r in reader:
            row = Row(
                path=r["path"],
                decode_ms=float(r["decode_ms"]) if r["decode_ms"] else 0.0,
                convert_ms=float(r["convert_ms"]) if r["convert_ms"] else 0.0,
                width=int(r["width"]) if r["width"] else 0,
                height=int(r["height"]) if r["height"] else 0,
                mse=float(r["mse"]) if r["mse"] else None,
                error=r["error"] if r["error"] else None,
            )
            rows[Path(r["path"]).name] = row
    return rows


def fmt_ms(ms: float) -> str:
    if ms >= 1000:
        return f"{ms / 1000:.2f}s"
    return f"{ms:.1f}ms"


def fmt_delta(delta: float, pct: float) -> str:
    sign = "+" if delta >= 0 else ""
    return f"{sign}{fmt_ms(delta)} ({sign}{pct:+.1f}%)"


def print_section(title: str) -> None:
    print(f"\n{'─' * 60}")
    print(f"  {title}")
    print(f"{'─' * 60}")


def cmd_diff(args: argparse.Namespace) -> None:
    a = load_csv(args.baseline)
    b = load_csv(args.compare)

    common = set(a) & set(b)
    only_baseline = set(a) - set(b)
    only_compare = set(b) - set(a)

    regressions = []
    improvements = []
    new_errors = []
    fixed_errors = []

    for name in common:
        ra, rb = a[name], b[name]

        if not ra.error and rb.error:
            new_errors.append((name, ra, rb))
        elif ra.error and not rb.error:
            fixed_errors.append((name, ra, rb))

        if ra.decode_ms > 0:
            delta = rb.decode_ms - ra.decode_ms
            pct = delta / ra.decode_ms * 100
            if abs(pct) >= args.threshold:
                entry = (name, ra, rb, delta, pct)
                if pct > 0:
                    regressions.append(entry)
                else:
                    improvements.append(entry)

    regressions.sort(key=lambda x: x[4], reverse=True)
    improvements.sort(key=lambda x: x[4])

    mse_regressions = []
    for name in common:
        ra, rb = a[name], b[name]
        if ra.mse is not None and rb.mse is not None:
            delta = rb.mse - ra.mse
            if delta > args.mse_delta:
                mse_regressions.append((name, ra, rb, delta))
    mse_regressions.sort(key=lambda x: x[3], reverse=True)

    print(f"\nBaseline : {args.baseline}")
    print(f"Compare  : {args.compare}")
    print(f"\nImages in baseline : {len(a)}")
    print(f"Images in compare  : {len(b)}")
    print(f"Common             : {len(common)}")
    if only_baseline:
        print(f"Only in baseline   : {len(only_baseline)}")
    if only_compare:
        print(f"Only in compare    : {len(only_compare)}")

    if new_errors:
        print_section(f"NEW ERRORS ({len(new_errors)})")
        for name, ra, rb in new_errors[:args.limit]:
            print(f"  {name}")
            print(f"    {rb.error}")
        if len(new_errors) > args.limit:
            print(f"  ... and {len(new_errors) - args.limit} more")

    if fixed_errors:
        print_section(f"FIXED ERRORS ({len(fixed_errors)})")
        for name, ra, rb in fixed_errors[:args.limit]:
            print(f"  {name}")
        if len(fixed_errors) > args.limit:
            print(f"  ... and {len(fixed_errors) - args.limit} more")

    if regressions:
        print_section(f"SPEED REGRESSIONS >{args.threshold}% ({len(regressions)})")
        for name, ra, rb, delta, pct in regressions[:args.limit]:
            print(f"  {name}")
            print(f"    {fmt_ms(ra.decode_ms)} → {fmt_ms(rb.decode_ms)}  {fmt_delta(delta, pct)}")
        if len(regressions) > args.limit:
            print(f"  ... and {len(regressions) - args.limit} more")

    if improvements:
        print_section(f"SPEED IMPROVEMENTS >{args.threshold}% ({len(improvements)})")
        for name, ra, rb, delta, pct in improvements[:args.limit]:
            print(f"  {name}")
            print(f"    {fmt_ms(ra.decode_ms)} → {fmt_ms(rb.decode_ms)}  {fmt_delta(delta, pct)}")
        if len(improvements) > args.limit:
            print(f"  ... and {len(improvements) - args.limit} more")

    if mse_regressions:
        print_section(f"MSE REGRESSIONS >+{args.mse_delta:.4f} ({len(mse_regressions)})")
        for name, ra, rb, delta in mse_regressions[:args.limit]:
            print(f"  {name}")
            print(f"    mse {ra.mse:.6f} → {rb.mse:.6f}  (+{delta:.6f})")
        if len(mse_regressions) > args.limit:
            print(f"  ... and {len(mse_regressions) - args.limit} more")

    if common:
        valid_a = [a[n].decode_ms for n in common if not a[n].error and a[n].decode_ms > 0]
        valid_b = [b[n].decode_ms for n in common if not b[n].error and b[n].decode_ms > 0]
        if valid_a and valid_b:
            avg_a = sum(valid_a) / len(valid_a)
            avg_b = sum(valid_b) / len(valid_b)
            total_a = sum(valid_a)
            total_b = sum(valid_b)
            print_section("AGGREGATE DECODE TIME (successful only)")
            print(f"  avg   {fmt_ms(avg_a)} → {fmt_ms(avg_b)}  ({(avg_b - avg_a) / avg_a * 100:+.1f}%)")
            print(f"  total {fmt_ms(total_a)} → {fmt_ms(total_b)}  ({(total_b - total_a) / total_a * 100:+.1f}%)")

        mse_a = [a[n].mse for n in common if a[n].mse is not None]
        mse_b = [b[n].mse for n in common if b[n].mse is not None]
        if mse_a and mse_b:
            avg_mse_a = sum(mse_a) / len(mse_a)
            avg_mse_b = sum(mse_b) / len(mse_b)
            print_section("AGGREGATE MSE")
            print(f"  avg mse  {avg_mse_a:.6f} → {avg_mse_b:.6f}  ({(avg_mse_b - avg_mse_a) / avg_mse_a * 100:+.2f}%)")


def cmd_analyze(args: argparse.Namespace) -> None:
    rows_map = load_csv(args.file)
    rows = list(rows_map.values())

    if args.top_slow:
        successful = [r for r in rows if not r.error and r.decode_ms > 0]
        successful.sort(key=lambda r: r.decode_ms, reverse=True)
        print_section(f"TOP {args.top_slow} SLOWEST DECODES")
        for r in successful[:args.top_slow]:
            ratio = r.decode_ms / r.convert_ms if r.convert_ms > 0 else float("inf")
            print(f"  {Path(r.path).name}")
            print(f"    decode={fmt_ms(r.decode_ms)}  ref={fmt_ms(r.convert_ms)}  ratio={ratio:.2f}x  size={r.width}x{r.height}")

    if args.mse_above is not None:
        high_mse = [r for r in rows if r.mse is not None and r.mse > args.mse_above]
        high_mse.sort(key=lambda r: r.mse, reverse=True)
        print_section(f"MSE ABOVE {args.mse_above} ({len(high_mse)} images)")
        for r in high_mse[:args.limit]:
            print(f"  {Path(r.path).name}")
            print(f"    mse={r.mse:.6f}  size={r.width}x{r.height}")
        if len(high_mse) > args.limit:
            print(f"  ... and {len(high_mse) - args.limit} more (use --limit to show more)")

    if args.errors:
        error_rows = [r for r in rows if r.error]
        print_section(f"ERRORS ({len(error_rows)} images)")
        for r in error_rows[:args.limit]:
            print(f"  {Path(r.path).name}")
            print(f"    {r.error}")
        if len(error_rows) > args.limit:
            print(f"  ... and {len(error_rows) - args.limit} more")

    if args.stats or not any([args.top_slow, args.mse_above is not None, args.errors]):
        successful = [r for r in rows if not r.error and r.decode_ms > 0]
        error_rows = [r for r in rows if r.error]
        mse_rows = [r for r in rows if r.mse is not None]

        print_section("SUMMARY STATS")
        print(f"  total images : {len(rows)}")
        print(f"  successful   : {len(successful)}")
        print(f"  errors       : {len(error_rows)}")

        if successful:
            times = [r.decode_ms for r in successful]
            print(f"\n  decode time (ms)")
            print(f"    min   {fmt_ms(min(times))}")
            print(f"    max   {fmt_ms(max(times))}")
            print(f"    avg   {fmt_ms(sum(times) / len(times))}")
            print(f"    total {fmt_ms(sum(times))}")
            times_sorted = sorted(times)
            p50 = times_sorted[len(times_sorted) // 2]
            p95 = times_sorted[int(len(times_sorted) * 0.95)]
            p99 = times_sorted[int(len(times_sorted) * 0.99)]
            print(f"    p50   {fmt_ms(p50)}")
            print(f"    p95   {fmt_ms(p95)}")
            print(f"    p99   {fmt_ms(p99)}")

            ref_valid = [r for r in successful if r.convert_ms > 0]
            if ref_valid:
                ratios = [r.decode_ms / r.convert_ms for r in ref_valid]
                faster = sum(1 for x in ratios if x < 1)
                print(f"\n  vs imagemagick (on {len(ref_valid)} images with ref data)")
                print(f"    avg ratio  {sum(ratios) / len(ratios):.2f}x  (vexel/imagemagick)")
                print(f"    faster     {faster} ({faster / len(ref_valid) * 100:.1f}%)")
                print(f"    slower     {len(ref_valid) - faster} ({(len(ref_valid) - faster) / len(ref_valid) * 100:.1f}%)")

        if mse_rows:
            mse_vals = [r.mse for r in mse_rows]
            print(f"\n  mse (on {len(mse_rows)} images)")
            print(f"    min   {min(mse_vals):.6f}")
            print(f"    max   {max(mse_vals):.6f}")
            print(f"    avg   {sum(mse_vals) / len(mse_vals):.6f}")

        if successful:
            slowest = sorted(successful, key=lambda r: r.decode_ms, reverse=True)[:10]
            print(f"\n  top 10 slowest")
            for r in slowest:
                ratio = r.decode_ms / r.convert_ms if r.convert_ms > 0 else float("inf")
                print(f"    {fmt_ms(r.decode_ms):>8}  {Path(r.path).name}  ({r.width}x{r.height}, ratio={ratio:.2f}x)")


RESULTS_DIR = Path(__file__).parent.parent / "corpus-results"
CSV_PATTERN = "corpus_bench_*.csv"


def find_latest_csvs(n: int) -> list[Path]:
    files = sorted(RESULTS_DIR.glob(CSV_PATTERN), key=lambda p: int(p.stem.split("_")[-1]))
    if len(files) < n:
        raise SystemExit(f"error: need at least {n} CSV(s) in {RESULTS_DIR}, found {len(files)}")
    return files[-n:]


def main() -> None:
    parser = argparse.ArgumentParser(
        description="Analyze and diff corpus benchmark CSV results.",
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog="""
examples:
  %(prog)s analyze
  %(prog)s analyze results.csv --stats
  %(prog)s analyze results.csv --top-slow 20
  %(prog)s analyze results.csv --mse-above 0.1
  %(prog)s analyze results.csv --errors
  %(prog)s diff
  %(prog)s diff baseline.csv new.csv
  %(prog)s diff baseline.csv new.csv --threshold 10 --mse-delta 0.01
""",
    )
    sub = parser.add_subparsers(dest="cmd", required=True)

    p_diff = sub.add_parser("diff", help="compare two corpus result CSVs")
    p_diff.add_argument("baseline", nargs="?", help="baseline CSV (default: second-latest in corpus-results/)")
    p_diff.add_argument("compare", nargs="?", help="comparison CSV (default: latest in corpus-results/)")
    p_diff.add_argument("--limit", type=int, default=20, metavar="N", help="max rows to show per section (default: 20)")
    p_diff.add_argument(
        "--threshold",
        type=float,
        default=20.0,
        metavar="PCT",
        help="minimum %% change in decode_ms to report (default: 20)",
    )
    p_diff.add_argument(
        "--mse-delta",
        type=float,
        default=0.005,
        metavar="DELTA",
        help="minimum MSE increase to report as regression (default: 0.005)",
    )

    p_analyze = sub.add_parser("analyze", help="analyze a single corpus result CSV")
    p_analyze.add_argument("file", nargs="?", help="CSV file to analyze (default: latest in corpus-results/)")
    p_analyze.add_argument("--limit", type=int, default=20, metavar="N", help="max rows to show per section (default: 20)")
    p_analyze.add_argument("--stats", action="store_true", help="show aggregate stats (default when no filter given)")
    p_analyze.add_argument("--top-slow", type=int, metavar="N", help="show N slowest decodes")
    p_analyze.add_argument("--mse-above", type=float, metavar="MSE", help="show images with MSE above threshold")
    p_analyze.add_argument("--errors", action="store_true", help="show all decode errors")

    args = parser.parse_args()

    if args.cmd == "diff":
        if args.baseline is None or args.compare is None:
            latest = find_latest_csvs(2)
            args.baseline = args.baseline or str(latest[0])
            args.compare = args.compare or str(latest[1])
        cmd_diff(args)
    elif args.cmd == "analyze":
        if args.file is None:
            args.file = str(find_latest_csvs(1)[0])
        cmd_analyze(args)


if __name__ == "__main__":
    main()
