# ctxvault Retrieval & Indexing Benchmark Report

- **Benchmark**: `swe_bench`
- **Repository**: `pallets/flask`
- **Evaluated Queries**: 3
- **Evaluation Cutoff**: K = 10

## 2. Retrieval Algorithm Quality & Latency Ablation

All IR metrics computed at cutoff **K = 10**:

| Mode | Recall@K | Precision@K | MRR@K | NDCG@K | Sep Ratio | Latency p50 | Latency p95 | Latency p99 | QPS |
|---|---|---|---|---|---|---|---|---|---|
| `bm25` | **0.333** | 0.033 | **0.333** | **0.333** | 1.40x | 69.83ms | 70.55ms | 70.61ms | **17** |
| `binary` | **0.000** | 0.000 | **0.000** | **0.000** | 1.12x | 3.30ms | 7.34ms | 7.70ms | **244** |
| `ppr` | **0.667** | 0.067 | **0.389** | **0.452** | 15.99x | 50.58ms | 64.40ms | 65.63ms | **21** |
| `fast` | **1.000** | 0.100 | **0.418** | **0.545** | 1.23x | 64.41ms | 98.98ms | 102.05ms | **14** |

### Metric Descriptions
- **Recall@K**: Fraction of ground-truth relevant documents retrieved in the top K.
- **MRR@K**: Mean Reciprocal Rank (1/rank of first relevant result).
- **NDCG@K**: Normalized Discounted Cumulative Gain accounting for graded relevance.
- **Sep Ratio**: Score separation between top-1 hit and bottom top-K hit (confidence margin).
- **QPS**: Sustained queries per second.

