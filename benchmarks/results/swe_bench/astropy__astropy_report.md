# ctxvault Retrieval & Indexing Benchmark Report

- **Benchmark**: `swe_bench`
- **Repository**: `astropy/astropy`
- **Evaluated Queries**: 6
- **Evaluation Cutoff**: K = 10

## 2. Retrieval Algorithm Quality & Latency Ablation

All IR metrics computed at cutoff **K = 10**:

| Mode | Recall@K | Precision@K | MRR@K | NDCG@K | Sep Ratio | Latency p50 | Latency p95 | Latency p99 | QPS |
|---|---|---|---|---|---|---|---|---|---|
| `bm25` | **1.000** | 0.100 | **1.000** | **1.000** | 2.13x | 341.53ms | 418.68ms | 424.24ms | **3** |
| `binary` | **0.333** | 0.033 | **0.089** | **0.148** | 1.05x | 9.55ms | 10.81ms | 11.03ms | **108** |
| `ppr` | **1.000** | 0.100 | **0.875** | **0.908** | 27.01x | 335.89ms | 420.43ms | 425.35ms | **3** |
| `fast` | **1.000** | 0.100 | **0.688** | **0.765** | 1.63x | 475.99ms | 541.15ms | 544.85ms | **2** |

### Metric Descriptions
- **Recall@K**: Fraction of ground-truth relevant documents retrieved in the top K.
- **MRR@K**: Mean Reciprocal Rank (1/rank of first relevant result).
- **NDCG@K**: Normalized Discounted Cumulative Gain accounting for graded relevance.
- **Sep Ratio**: Score separation between top-1 hit and bottom top-K hit (confidence margin).
- **QPS**: Sustained queries per second.

