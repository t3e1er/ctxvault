# ctxvault Comprehensive Benchmark Master Summary

- **Aggregated Sub-Reports**: 11 evaluation suites
- **Total Configurations**: 44 benchmark rows

## 1. Master Leaderboard

| Benchmark | Repository | Mode | Recall@K | MRR@K | NDCG@K | Sep Ratio | Latency p50 | QPS |
|---|---|---|---|---|---|---|---|---|
| `codesearchnet` | `coreos/go-omaha` | `bm25` | **1.000** | **1.000** | **1.000** | 1.75x | 4.29ms | **201** |
| `codesearchnet` | `coreos/go-omaha` | `binary` | **0.000** | **0.000** | **0.000** | 1.14x | 0.14ms | **7187** |
| `codesearchnet` | `coreos/go-omaha` | `ppr` | **1.000** | **1.000** | **1.000** | 45.75x | 7.85ms | **135** |
| `codesearchnet` | `coreos/go-omaha` | `fast` | **1.000** | **1.000** | **1.000** | 1.22x | 7.99ms | **129** |
| `codesearchnet` | `grokify/gotilla` | `bm25` | **0.000** | **0.000** | **0.000** | 1.52x | 93.26ms | **11** |
| `codesearchnet` | `grokify/gotilla` | `binary` | **0.000** | **0.000** | **0.000** | 1.08x | 1.06ms | **942** |
| `codesearchnet` | `grokify/gotilla` | `ppr` | **0.500** | **0.250** | **0.316** | 26.43x | 87.68ms | **11** |
| `codesearchnet` | `grokify/gotilla` | `fast` | **0.500** | **0.056** | **0.150** | 1.19x | 90.59ms | **11** |
| `codesearchnet` | `tylertreat/BoomFilters` | `bm25` | **1.000** | **1.000** | **1.000** | 1.59x | 5.70ms | **176** |
| `codesearchnet` | `tylertreat/BoomFilters` | `binary` | **1.000** | **1.000** | **1.000** | 1.20x | 0.10ms | **9891** |
| `codesearchnet` | `tylertreat/BoomFilters` | `ppr` | **1.000** | **1.000** | **1.000** | 21.76x | 3.03ms | **330** |
| `codesearchnet` | `tylertreat/BoomFilters` | `fast` | **1.000** | **1.000** | **1.000** | 1.17x | 6.76ms | **148** |
| `codesearchnet` | `wcharczuk/go-chart` | `bm25` | **0.500** | **0.500** | **0.500** | 1.68x | 13.81ms | **72** |
| `codesearchnet` | `wcharczuk/go-chart` | `binary` | **0.500** | **0.125** | **0.215** | 1.18x | 1.09ms | **916** |
| `codesearchnet` | `wcharczuk/go-chart` | `ppr` | **0.500** | **0.500** | **0.500** | 48.44x | 15.29ms | **65** |
| `codesearchnet` | `wcharczuk/go-chart` | `fast` | **0.500** | **0.500** | **0.500** | 1.32x | 14.36ms | **70** |
| `repobench` | `DLYuanGod/TinyGPT-V` | `bm25` | **1.000** | **0.111** | **0.301** | 1.13x | 29.18ms | **34** |
| `repobench` | `DLYuanGod/TinyGPT-V` | `binary` | **0.000** | **0.000** | **0.000** | 1.04x | 0.32ms | **3154** |
| `repobench` | `DLYuanGod/TinyGPT-V` | `ppr` | **1.000** | **0.200** | **0.387** | 17.91x | 33.37ms | **30** |
| `repobench` | `DLYuanGod/TinyGPT-V` | `fast` | **1.000** | **0.250** | **0.431** | 2.05x | 34.30ms | **29** |
| `repobench` | `Meituan-AutoML/MobileVLM` | `bm25` | **1.000** | **1.000** | **1.000** | 1.90x | 7.43ms | **134** |
| `repobench` | `Meituan-AutoML/MobileVLM` | `binary` | **1.000** | **0.500** | **0.631** | 1.14x | 0.25ms | **3976** |
| `repobench` | `Meituan-AutoML/MobileVLM` | `ppr` | **1.000** | **1.000** | **1.000** | 20.05x | 5.75ms | **174** |
| `repobench` | `Meituan-AutoML/MobileVLM` | `fast` | **1.000** | **1.000** | **1.000** | 1.23x | 6.96ms | **144** |
| `repobench` | `ali-vilab/dreamtalk` | `bm25` | **1.000** | **0.250** | **0.431** | 1.83x | 4.64ms | **216** |
| `repobench` | `ali-vilab/dreamtalk` | `binary` | **0.000** | **0.000** | **0.000** | 1.08x | 0.11ms | **8865** |
| `repobench` | `ali-vilab/dreamtalk` | `ppr` | **1.000** | **0.500** | **0.631** | 41.58x | 4.83ms | **207** |
| `repobench` | `ali-vilab/dreamtalk` | `fast` | **1.000** | **0.500** | **0.631** | 1.24x | 4.81ms | **208** |
| `repobench` | `jianchang512/vocal-separate` | `bm25` | **1.000** | **1.000** | **1.000** | 3.11x | 2.99ms | **334** |
| `repobench` | `jianchang512/vocal-separate` | `binary` | **0.000** | **0.000** | **0.000** | 1.16x | 0.20ms | **4926** |
| `repobench` | `jianchang512/vocal-separate` | `ppr` | **1.000** | **1.000** | **1.000** | 43.95x | 2.35ms | **426** |
| `repobench` | `jianchang512/vocal-separate` | `fast` | **1.000** | **0.500** | **0.631** | 1.59x | 6.15ms | **163** |
| `swe_bench` | `astropy/astropy` | `bm25` | **1.000** | **1.000** | **1.000** | 2.12x | 183.46ms | **5** |
| `swe_bench` | `astropy/astropy` | `binary` | **0.000** | **0.000** | **0.000** | 1.06x | 4.09ms | **244** |
| `swe_bench` | `astropy/astropy` | `ppr` | **1.000** | **0.875** | **0.908** | 27.00x | 176.71ms | **6** |
| `swe_bench` | `astropy/astropy` | `fast` | **1.000** | **1.000** | **1.000** | 1.22x | 327.76ms | **3** |
| `swe_bench` | `pallets/flask` | `bm25` | **0.333** | **0.333** | **0.333** | 1.40x | 18.71ms | **50** |
| `swe_bench` | `pallets/flask` | `binary` | **0.000** | **0.000** | **0.000** | 1.12x | 1.02ms | **1133** |
| `swe_bench` | `pallets/flask` | `ppr` | **0.667** | **0.389** | **0.452** | 15.99x | 23.18ms | **50** |
| `swe_bench` | `pallets/flask` | `fast` | **1.000** | **0.418** | **0.545** | 1.23x | 28.48ms | **35** |
| `swe_bench` | `psf/requests` | `bm25` | **0.800** | **0.540** | **0.604** | 1.40x | 12.79ms | **63** |
| `swe_bench` | `psf/requests` | `binary` | **0.833** | **0.321** | **0.437** | 1.11x | 0.43ms | **1514** |
| `swe_bench` | `psf/requests` | `ppr` | **1.000** | **0.507** | **0.630** | 83.38x | 28.70ms | **34** |
| `swe_bench` | `psf/requests` | `fast` | **1.000** | **0.500** | **0.624** | 1.20x | 25.82ms | **31** |

## 2. Mode Macro-Averages

| Mode | Configurations | Avg Recall@K | Avg MRR@K | Avg NDCG@K | Avg Sep Ratio | Avg Latency p50 | Avg QPS |
|---|---|---|---|---|---|---|---|
| `bm25` | 11 | **0.785** | **0.612** | **0.652** | 1.77x | 34.20ms | **118** |
| `binary` | 11 | **0.303** | **0.177** | **0.208** | 1.12x | 0.80ms | **3886** |
| `ppr` | 11 | **0.879** | **0.656** | **0.711** | 35.66x | 35.34ms | **134** |
| `fast` | 11 | **0.909** | **0.611** | **0.683** | 1.33x | 50.36ms | **88** |

### Metric Descriptions
- **Recall@K**: Fraction of ground-truth target files retrieved in top K.
- **MRR@K**: Mean Reciprocal Rank of first relevant document.
- **NDCG@K**: Normalized Discounted Cumulative Gain accounting for graded relevance.
- **Sep Ratio**: Score separation confidence margin between top-1 hit and bottom top-K hit.
- **QPS**: Measured sustained queries per second.

