# ctxvault Retrieval & Indexing Benchmark Report

- **Benchmark**: `swe_bench`
- **Repository**: `astropy/astropy`
- **Evaluated Queries**: 6
- **Evaluation Cutoff**: K = 10

## 2. Retrieval Algorithm Quality & Latency Ablation

All IR metrics computed at cutoff **K = 10**:

| Mode | Recall@K | Precision@K | MRR@K | NDCG@K | Sep Ratio | Latency p50 | Latency p95 | Latency p99 | QPS |
|---|---|---|---|---|---|---|---|---|---|
| `bm25` | **1.000** | 0.100 | **1.000** | **1.000** | 2.18x | 390.22ms | 661.95ms | 693.26ms | **2** |
| `binary` | **0.000** | 0.000 | **0.000** | **0.000** | 1.06x | 10.96ms | 12.20ms | 12.32ms | **97** |
| `ppr` | **1.000** | 0.100 | **0.875** | **0.908** | 34.66x | 556.55ms | 754.39ms | 778.83ms | **2** |
| `fast` | **1.000** | 0.100 | **1.000** | **1.000** | 1.22x | 878.28ms | 984.27ms | 991.29ms | **1** |

### Metric Descriptions
- **Recall@K**: Fraction of ground-truth relevant documents retrieved in the top K.
- **MRR@K**: Mean Reciprocal Rank (1/rank of first relevant result).
- **NDCG@K**: Normalized Discounted Cumulative Gain accounting for graded relevance.
- **Sep Ratio**: Score separation between top-1 hit and bottom top-K hit (confidence margin).
- **QPS**: Sustained queries per second.

