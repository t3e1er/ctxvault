# ctxvault Comprehensive Benchmark Master Summary

- **Aggregated Sub-Reports**: 11 evaluation suites
- **Total Configurations**: 44 benchmark rows

## 1. Master Leaderboard

| Benchmark | Repository | Mode | Recall@K | MRR@K | NDCG@K | Sep Ratio | Latency p50 | QPS |
|---|---|---|---|---|---|---|---|---|
| `codesearchnet` | `coreos/go-omaha` | `bm25` | **1.000** | **1.000** | **1.000** | 1.75x | 6.88ms | **145** |
| `codesearchnet` | `coreos/go-omaha` | `binary` | **1.000** | **0.200** | **0.387** | 1.25x | 0.42ms | **2398** |
| `codesearchnet` | `coreos/go-omaha` | `ppr` | **1.000** | **1.000** | **1.000** | 45.75x | 5.16ms | **200** |
| `codesearchnet` | `coreos/go-omaha` | `fast` | **1.000** | **1.000** | **1.000** | 3.28x | 11.35ms | **88** |
| `codesearchnet` | `grokify/gotilla` | `bm25` | **0.750** | **0.250** | **0.331** | 1.99x | 40.41ms | **25** |
| `codesearchnet` | `grokify/gotilla` | `binary` | **0.250** | **0.062** | **0.097** | 1.09x | 0.95ms | **1051** |
| `codesearchnet` | `grokify/gotilla` | `ppr` | **0.750** | **0.550** | **0.589** | 11.72x | 39.53ms | **25** |
| `codesearchnet` | `grokify/gotilla` | `fast` | **0.750** | **0.350** | **0.387** | 1.71x | 46.44ms | **22** |
| `codesearchnet` | `tylertreat/BoomFilters` | `bm25` | **1.000** | **1.000** | **1.000** | 1.56x | 5.60ms | **179** |
| `codesearchnet` | `tylertreat/BoomFilters` | `binary` | **1.000** | **1.000** | **1.000** | 1.20x | 0.56ms | **1781** |
| `codesearchnet` | `tylertreat/BoomFilters` | `ppr` | **1.000** | **1.000** | **1.000** | 21.62x | 5.31ms | **188** |
| `codesearchnet` | `tylertreat/BoomFilters` | `fast` | **1.000** | **1.000** | **1.000** | 1.72x | 7.97ms | **125** |
| `codesearchnet` | `wcharczuk/go-chart` | `bm25` | **0.500** | **0.500** | **0.500** | 1.68x | 20.25ms | **49** |
| `codesearchnet` | `wcharczuk/go-chart` | `binary` | **0.500** | **0.500** | **0.500** | 1.15x | 0.62ms | **1599** |
| `codesearchnet` | `wcharczuk/go-chart` | `ppr` | **0.500** | **0.500** | **0.500** | 48.46x | 19.06ms | **52** |
| `codesearchnet` | `wcharczuk/go-chart` | `fast` | **0.500** | **0.500** | **0.500** | 1.85x | 24.20ms | **41** |
| `repobench` | `DLYuanGod/TinyGPT-V` | `bm25` | **1.000** | **0.333** | **0.500** | 1.13x | 13.56ms | **74** |
| `repobench` | `DLYuanGod/TinyGPT-V` | `binary` | **1.000** | **0.200** | **0.387** | 1.05x | 0.81ms | **1231** |
| `repobench` | `DLYuanGod/TinyGPT-V` | `ppr` | **1.000** | **0.100** | **0.289** | 3.78x | 11.19ms | **89** |
| `repobench` | `DLYuanGod/TinyGPT-V` | `fast` | **1.000** | **0.500** | **0.631** | 2.04x | 14.69ms | **68** |
| `repobench` | `Meituan-AutoML/MobileVLM` | `bm25` | **1.000** | **1.000** | **1.000** | 1.90x | 10.75ms | **93** |
| `repobench` | `Meituan-AutoML/MobileVLM` | `binary` | **1.000** | **0.250** | **0.431** | 1.19x | 1.66ms | **602** |
| `repobench` | `Meituan-AutoML/MobileVLM` | `ppr` | **1.000** | **1.000** | **1.000** | 20.05x | 10.63ms | **94** |
| `repobench` | `Meituan-AutoML/MobileVLM` | `fast` | **1.000** | **1.000** | **1.000** | 1.81x | 16.86ms | **59** |
| `repobench` | `ali-vilab/dreamtalk` | `bm25` | **1.000** | **0.250** | **0.431** | 1.83x | 8.49ms | **118** |
| `repobench` | `ali-vilab/dreamtalk` | `binary` | **1.000** | **0.333** | **0.500** | 1.19x | 0.81ms | **1227** |
| `repobench` | `ali-vilab/dreamtalk` | `ppr` | **1.000** | **0.500** | **0.631** | 42.84x | 6.69ms | **149** |
| `repobench` | `ali-vilab/dreamtalk` | `fast` | **1.000** | **0.500** | **0.631** | 1.81x | 11.03ms | **91** |
| `repobench` | `jianchang512/vocal-separate` | `bm25` | **1.000** | **1.000** | **1.000** | 3.11x | 6.80ms | **147** |
| `repobench` | `jianchang512/vocal-separate` | `binary` | **1.000** | **0.333** | **0.500** | 1.38x | 1.95ms | **513** |
| `repobench` | `jianchang512/vocal-separate` | `ppr` | **1.000** | **1.000** | **1.000** | 43.95x | 7.93ms | **126** |
| `repobench` | `jianchang512/vocal-separate` | `fast` | **1.000** | **1.000** | **1.000** | 3.30x | 9.75ms | **102** |
| `swe_bench` | `astropy/astropy` | `bm25` | **1.000** | **1.000** | **1.000** | 2.13x | 341.53ms | **3** |
| `swe_bench` | `astropy/astropy` | `binary` | **0.333** | **0.089** | **0.148** | 1.05x | 9.55ms | **108** |
| `swe_bench` | `astropy/astropy` | `ppr` | **1.000** | **0.875** | **0.908** | 27.01x | 335.89ms | **3** |
| `swe_bench` | `astropy/astropy` | `fast` | **1.000** | **0.688** | **0.765** | 1.63x | 475.99ms | **2** |
| `swe_bench` | `pallets/flask` | `bm25` | **0.333** | **0.333** | **0.333** | 1.46x | 28.12ms | **40** |
| `swe_bench` | `pallets/flask` | `binary` | **0.667** | **0.194** | **0.310** | 1.09x | 5.85ms | **233** |
| `swe_bench` | `pallets/flask` | `ppr` | **0.667** | **0.417** | **0.477** | 14.09x | 27.31ms | **41** |
| `swe_bench` | `pallets/flask` | `fast` | **0.667** | **0.417** | **0.477** | 1.72x | 38.22ms | **28** |
| `swe_bench` | `psf/requests` | `bm25` | **0.800** | **0.540** | **0.604** | 1.46x | 23.00ms | **42** |
| `swe_bench` | `psf/requests` | `binary` | **0.833** | **0.275** | **0.399** | 1.16x | 2.85ms | **287** |
| `swe_bench` | `psf/requests` | `ppr` | **0.800** | **0.333** | **0.452** | 133.91x | 19.03ms | **48** |
| `swe_bench` | `psf/requests` | `fast` | **1.000** | **0.383** | **0.538** | 1.74x | 34.78ms | **29** |

## 2. Mode Macro-Averages

| Mode | Configurations | Avg Recall@K | Avg MRR@K | Avg NDCG@K | Avg Sep Ratio | Avg Latency p50 | Avg QPS |
|---|---|---|---|---|---|---|---|
| `bm25` | 11 | **0.853** | **0.655** | **0.700** | 1.82x | 45.94ms | **83** |
| `binary` | 11 | **0.780** | **0.312** | **0.423** | 1.16x | 2.37ms | **1003** |
| `ppr` | 11 | **0.883** | **0.661** | **0.713** | 37.56x | 44.34ms | **92** |
| `fast` | 11 | **0.902** | **0.667** | **0.721** | 2.06x | 62.84ms | **60** |

### Metric Descriptions
- **Recall@K**: Fraction of ground-truth target files retrieved in top K.
- **MRR@K**: Mean Reciprocal Rank of first relevant document.
- **NDCG@K**: Normalized Discounted Cumulative Gain accounting for graded relevance.
- **Sep Ratio**: Score separation confidence margin between top-1 hit and bottom top-K hit.
- **QPS**: Measured sustained queries per second.

