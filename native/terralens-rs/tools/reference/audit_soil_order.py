#!/usr/bin/env python3
"""Read-only soil-decoration order audit for terralens-surface-mismatch-audit.json.

Enumerates whole source-chunk decoration orders. Within each source chunk, soil
features remain in ascending FeatureSorter index, as in TFC applyBiomeDecoration.
The output path is explicit; no Minecraft saves are opened or changed.
"""
import argparse, collections, gzip, itertools, json
from pathlib import Path

DEFAULT_REPORT = Path(__file__).resolve().parents[4]/'docs/data/surface-mismatch-world0-before.json.gz'
DEFAULT_SOILS = Path(__file__).resolve().parents[2]/'src/ground/decorations/assets/soils.json'
DEFAULT_OUTPUT = Path('/tmp/terralens-soil-order-permutation-proof.json')

def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('--report', type=Path, default=DEFAULT_REPORT)
    ap.add_argument('--soils', type=Path, default=DEFAULT_SOILS)
    ap.add_argument('--output', type=Path, default=DEFAULT_OUTPUT)
    args = ap.parse_args()
    report = json.loads(gzip.decompress(args.report.read_bytes())) if args.report.suffix=='.gz' else json.loads(args.report.read_text())
    soil_data = json.loads(args.soils.read_text())
    states = {f['index']: f['states'] for f in soil_data['features']}
    rows_out = []
    totals = collections.Counter()
    by_pair = collections.defaultdict(collections.Counter)
    by_pair_chunks = collections.defaultdict(collections.Counter)
    target = {
      'tfg:grass/podzol -> tfg:duff/podzol',
      'tfg:grass/alfisol -> tfg:duff/alfisol',
      'tfg:grass/podzol -> tfg:duff/alfisol',
    }
    for row in report['surface_rows']:
        x, z = row['actual']['x'], row['actual']['z']
        actual = row['actual']['ground_block']
        reported = row['predicted_block_or_fluid']
        base = row['trace']['base']
        pair = f'{actual} -> {reported}'
        raw = row['trace']['decoration_candidates']['soils']
        candidates = []
        for order, c in enumerate(raw):
            ox, oy, oz = c['origin']
            dx, dz = x - ox, z - oz
            # Matches ground/decorations/mod.rs patch disk and height gates.
            if dx*dx + dz*dz > c['radius']*c['radius']:
                continue
            if abs(base['y'] - oy) > c['height']:
                continue
            candidates.append((order, c))
        groups = collections.defaultdict(list)
        for order, c in candidates:
            groups[tuple(c['source_chunk'])].append((order, c))
        chunks = sorted(groups)

        def apply_order(chunk_order):
            block = base['block']
            for chunk in chunk_order:
                # preserve same-index placement sequence from trace; feature index
                # order within one source chunk follows Java's sortedIndices loop.
                for _, c in sorted(groups[chunk], key=lambda pair: (pair[1]['index'], pair[0])):
                    block = states[c['index']].get(block, block)
            return block

        # The current Rust projection globally sorts patches by index, stable for ties.
        global_block = base['block']
        for _, c in sorted(candidates, key=lambda pair: (pair[1]['index'], pair[0])):
            global_block = states[c['index']].get(global_block, global_block)
        permutations = list(itertools.permutations(chunks)) if chunks else [tuple()]
        witnesses = []
        reachable_outputs = set()
        for permutation in permutations:
            output = apply_order(permutation)
            reachable_outputs.add(output)
            if output == actual:
                witnesses.append([list(c) for c in permutation])
        actual_reachable = actual in reachable_outputs
        global_actual = global_block == actual
        multi_chunk_alternative = actual_reachable and not global_actual and len(chunks) >= 2
        if not candidates:
            reason = 'no_soil_candidates'
        elif actual_reachable:
            reason = 'reachable_by_source_chunk_order'
        elif len(chunks) == 1:
            reason = 'unresolved_same_source_chunk_order'
        else:
            reason = 'unresolved_even_after_cross_source_chunk_permutation'
        if not candidates:
            classification = 'no_soil_candidates'
        elif global_actual:
            classification = 'actual_matches_imposed_global_soil_order'
        elif multi_chunk_alternative:
            classification = 'cross_chunk_order_explains_actual'
        elif len(chunks) == 1:
            classification = 'unresolved_same_source_chunk_order'
        else:
            classification = 'unresolved_cross_source_chunk_order'
        item = {
            'x': x, 'z': z, 'pair': pair,
            'base_block': base['block'], 'actual_block': actual,
            'reported_prediction': reported,
            'soil_global_index_result': global_block,
            'soil_global_matches_report': global_block == reported,
            'source_chunks': [list(c) for c in chunks],
            'source_chunk_count': len(chunks),
            'candidate_soils': [c for _, c in candidates],
            'reachable_soil_outputs': sorted(reachable_outputs),
            'actual_reachable_by_source_chunk_order': actual_reachable,
            'global_order_already_actual': global_actual,
            'cross_chunk_order_changes_global_to_actual': multi_chunk_alternative,
            'order_only_explanation': multi_chunk_alternative and global_block==reported and row['trace']['base']==row['trace']['before_forest'],
            'actual_witness_orders': witnesses,
            'unreachable_class': reason,
            'exclusive_classification': classification,
        }
        rows_out.append(item)
        totals['rows'] += 1
        totals['order_only_explanation'] += item['order_only_explanation']
        totals['exclusive_' + classification] += 1
        totals['reported_prediction_matches_soil_global_result'] += item['soil_global_matches_report']
        totals['actual_reachable_by_any_order'] += actual_reachable
        totals['actual_already_matches_soil_global_result'] += global_actual
        totals['cross_chunk_order_changes_global_to_actual'] += multi_chunk_alternative
        totals[reason] += 1
        if reason == 'unresolved_same_source_chunk_order': totals['unresolved_same_chunk_rows'] += 1
        by_pair[pair]['rows'] += 1
        by_pair[pair][reason] += 1
        by_pair_chunks[pair][str(len(chunks))] += 1
        if item['soil_global_matches_report']: by_pair[pair]['soil_global_matches_report'] += 1
        if multi_chunk_alternative: by_pair[pair]['cross_chunk_order_changes_global_to_actual'] += 1
        if actual_reachable: by_pair[pair]['actual_reachable_by_any_order'] += 1
        if global_actual: by_pair[pair]['actual_already_matches_soil_global_result'] += 1

    result = {
      'method': 'Exhaustive permutations of whole source-chunk groups, preserving ascending placed-feature index within each chunk; configured replacement maps from soil profile.',
      'source_files': {'trace': str(args.report), 'soil_profile': str(args.soils)},
      'scope': 'Read-only simulation of stage-8 soil replacement patches only. This tests reachability under source-consistent per-chunk feature order; it does not recover the actual chunk scheduling history recorded in the saved world.',
      'geometry': 'Candidate center/radius circle and abs(base ground Y - placement origin Y) <= disc height, matching the soil patch application gate for these no-boulder target groups.',
      'totals': dict(totals),
      'target_big_duff_pairs': sorted(target),
      'target_big_duff_summary': {
        'rows': sum(by_pair[p]['rows'] for p in target),
        'cross_chunk_order_changes_global_to_actual': sum(by_pair[p]['cross_chunk_order_changes_global_to_actual'] for p in target),
        'actual_reachable_by_any_order': sum(by_pair[p]['actual_reachable_by_any_order'] for p in target),
        'unresolved_same_source_chunk_order': sum(by_pair[p]['unresolved_same_source_chunk_order'] for p in target),
        'source_chunk_count_distribution': dict(collections.Counter({})),
        'pairs': {p: dict(by_pair[p]) for p in sorted(target)},
      },
      'by_pair': {p: {**dict(stats), 'source_chunk_count_distribution': dict(by_pair_chunks[p])} for p, stats in sorted(by_pair.items())},
      'rows': rows_out,
    }
    # Recompute a proper aggregated source-count histogram for the target classes.
    histogram = collections.Counter()
    for item in rows_out:
        if item['pair'] in target:
            histogram[str(item['source_chunk_count'])] += 1
    result['target_big_duff_summary']['source_chunk_count_distribution'] = dict(sorted(histogram.items()))
    args.output.write_text(json.dumps(result, indent=2) + '\n')
    print(json.dumps({'output': str(args.output), 'totals': result['totals'], 'target_big_duff_summary': result['target_big_duff_summary']}, indent=2))

if __name__ == '__main__': main()
