# ctxvault Comprehensive Benchmark Master Summary

- **Aggregated Sub-Reports**: 9 evaluation suites
- **Total Configurations**: 36 benchmark rows

## 1. Master Leaderboard

| Benchmark | Repository | Mode | Recall@K | MRR@K | NDCG@K | Sep Ratio | Latency p50 | QPS |
|---|---|---|---|---|---|---|---|---|
| `codesearchnet` | `gin-gonic/gin` | `bm25` | **0.000** | **0.000** | **0.000** | 0.00x | 0.00ms | **0** |
| `codesearchnet` | `gin-gonic/gin` | `binary` | **0.000** | **0.000** | **0.000** | 0.00x | 0.00ms | **0** |
| `codesearchnet` | `gin-gonic/gin` | `ppr` | **0.000** | **0.000** | **0.000** | 0.00x | 0.00ms | **0** |
| `codesearchnet` | `gin-gonic/gin` | `fast` | **0.000** | **0.000** | **0.000** | 0.00x | 0.00ms | **0** |
| `codesearchnet` | `pallets/flask` | `bm25` | **0.000** | **0.000** | **0.000** | 0.00x | 0.00ms | **0** |
| `codesearchnet` | `pallets/flask` | `binary` | **0.000** | **0.000** | **0.000** | 0.00x | 0.00ms | **0** |
| `codesearchnet` | `pallets/flask` | `ppr` | **0.000** | **0.000** | **0.000** | 0.00x | 0.00ms | **0** |
| `codesearchnet` | `pallets/flask` | `fast` | **0.000** | **0.000** | **0.000** | 0.00x | 0.00ms | **0** |
| `repobench` | `DLYuanGod/TinyGPT-V` | `bm25` | **1.000** | **0.111** | **0.301** | 1.13x | 105.60ms | **10** |
| `repobench` | `DLYuanGod/TinyGPT-V` | `binary` | **0.000** | **0.000** | **0.000** | 1.04x | 3.99ms | **251** |
| `repobench` | `DLYuanGod/TinyGPT-V` | `ppr` | **1.000** | **0.200** | **0.387** | 17.91x | 97.82ms | **10** |
| `repobench` | `DLYuanGod/TinyGPT-V` | `fast` | **1.000** | **0.250** | **0.431** | 2.05x | 123.83ms | **8** |
| `repobench` | `Meituan-AutoML/MobileVLM` | `bm25` | **1.000** | **1.000** | **1.000** | 1.90x | 19.31ms | **52** |
| `repobench` | `Meituan-AutoML/MobileVLM` | `binary` | **0.000** | **0.000** | **0.000** | 1.09x | 1.32ms | **760** |
| `repobench` | `Meituan-AutoML/MobileVLM` | `ppr` | **1.000** | **1.000** | **1.000** | 20.05x | 20.14ms | **50** |
| `repobench` | `Meituan-AutoML/MobileVLM` | `fast` | **1.000** | **1.000** | **1.000** | 1.23x | 28.98ms | **34** |
| `repobench` | `ali-vilab/dreamtalk` | `bm25` | **1.000** | **0.250** | **0.431** | 1.83x | 45.28ms | **22** |
| `repobench` | `ali-vilab/dreamtalk` | `binary` | **0.000** | **0.000** | **0.000** | 1.08x | 0.73ms | **1362** |
| `repobench` | `ali-vilab/dreamtalk` | `ppr` | **1.000** | **0.500** | **0.631** | 41.62x | 29.74ms | **34** |
| `repobench` | `ali-vilab/dreamtalk` | `fast` | **1.000** | **0.500** | **0.631** | 1.23x | 18.10ms | **55** |
| `repobench` | `jianchang512/vocal-separate` | `bm25` | **1.000** | **1.000** | **1.000** | 3.11x | 12.20ms | **82** |
| `repobench` | `jianchang512/vocal-separate` | `binary` | **0.000** | **0.000** | **0.000** | 1.10x | 1.07ms | **933** |
| `repobench` | `jianchang512/vocal-separate` | `ppr` | **1.000** | **1.000** | **1.000** | 43.95x | 9.60ms | **104** |
| `repobench` | `jianchang512/vocal-separate` | `fast` | **1.000** | **0.500** | **0.631** | 1.59x | 14.40ms | **69** |
| `repobench` | `jiawei-ren/dreamgaussian4d` | `bm25` | **0.000** | **0.000** | **0.000** | 1.28x | 53.34ms | **19** |
| `repobench` | `jiawei-ren/dreamgaussian4d` | `binary` | **0.000** | **0.000** | **0.000** | 1.04x | 1.01ms | **988** |
| `repobench` | `jiawei-ren/dreamgaussian4d` | `ppr` | **0.000** | **0.000** | **0.000** | 4.47x | 88.00ms | **11** |
| `repobench` | `jiawei-ren/dreamgaussian4d` | `fast` | **0.000** | **0.000** | **0.000** | 1.19x | 90.14ms | **11** |
| `swe_bench` | `astropy/astropy` | `bm25` | **1.000** | **1.000** | **1.000** | 2.18x | 390.22ms | **2** |
| `swe_bench` | `astropy/astropy` | `binary` | **0.000** | **0.000** | **0.000** | 1.06x | 10.96ms | **97** |
| `swe_bench` | `astropy/astropy` | `ppr` | **1.000** | **0.875** | **0.908** | 34.66x | 556.55ms | **2** |
| `swe_bench` | `astropy/astropy` | `fast` | **1.000** | **1.000** | **1.000** | 1.22x | 878.28ms | **1** |
| `swe_bench` | `pallets/flask` | `bm25` | **0.333** | **0.333** | **0.333** | 1.40x | 69.83ms | **17** |
| `swe_bench` | `pallets/flask` | `binary` | **0.000** | **0.000** | **0.000** | 1.12x | 3.30ms | **244** |
| `swe_bench` | `pallets/flask` | `ppr` | **0.667** | **0.389** | **0.452** | 15.99x | 50.58ms | **21** |
| `swe_bench` | `pallets/flask` | `fast` | **1.000** | **0.418** | **0.545** | 1.23x | 64.41ms | **14** |

## 2. Mode Macro-Averages

| Mode | Configurations | Avg Recall@K | Avg MRR@K | Avg NDCG@K | Avg Sep Ratio | Avg Latency p50 | Avg QPS |
|---|---|---|---|---|---|---|---|
| `bm25` | 9 | **0.593** | **0.410** | **0.452** | 1.43x | 77.31ms | **23** |
| `binary` | 9 | **0.000** | **0.000** | **0.000** | 0.84x | 2.49ms | **515** |
| `ppr` | 9 | **0.630** | **0.440** | **0.486** | 19.85x | 94.71ms | **26** |
| `fast` | 9 | **0.667** | **0.408** | **0.471** | 1.08x | 135.35ms | **21** |

### Metric Descriptions
- **Recall@K**: Fraction of ground-truth target files retrieved in top K.
- **MRR@K**: Mean Reciprocal Rank of first relevant document.
- **NDCG@K**: Normalized Discounted Cumulative Gain accounting for graded relevance.
- **Sep Ratio**: Score separation confidence margin between top-1 hit and bottom top-K hit.
- **QPS**: Measured sustained queries per second.

