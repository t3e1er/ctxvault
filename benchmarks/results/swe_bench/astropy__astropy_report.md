# ctxvault Retrieval & Indexing Benchmark Report

- **Benchmark**: `swe_bench`
- **Repository**: `astropy/astropy`
- **Evaluated Queries**: 6
- **Evaluation Cutoff**: K = 10

## 2. Retrieval Algorithm Quality & Latency Ablation

All IR metrics computed at cutoff **K = 10**:

| Mode | Recall@K | Precision@K | MRR@K | NDCG@K | Sep Ratio | Latency p50 | Latency p95 | Latency p99 | QPS |
|---|---|---|---|---|---|---|---|---|---|
| `bm25` | **1.000** | 0.100 | **1.000** | **1.000** | 2.12x | 183.46ms | 340.72ms | 359.24ms | **5** |
| `binary` | **0.000** | 0.000 | **0.000** | **0.000** | 1.06x | 4.09ms | 5.87ms | 6.08ms | **244** |
| `ppr` | **1.000** | 0.100 | **0.875** | **0.908** | 27.00x | 176.71ms | 260.43ms | 268.73ms | **6** |
| `fast` | **1.000** | 0.100 | **1.000** | **1.000** | 1.22x | 327.76ms | 412.16ms | 418.00ms | **3** |

### Metric Descriptions
- **Recall@K**: Fraction of ground-truth relevant documents retrieved in the top K.
- **MRR@K**: Mean Reciprocal Rank (1/rank of first relevant result).
- **NDCG@K**: Normalized Discounted Cumulative Gain accounting for graded relevance.
- **Sep Ratio**: Score separation between top-1 hit and bottom top-K hit (confidence margin).
- **QPS**: Sustained queries per second.

