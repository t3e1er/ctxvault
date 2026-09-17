# ctxvault Retrieval & Indexing Benchmark Report

- **Benchmark**: `repobench`
- **Repository**: `ali-vilab/dreamtalk`
- **Evaluated Queries**: 1
- **Evaluation Cutoff**: K = 10

## 2. Retrieval Algorithm Quality & Latency Ablation

All IR metrics computed at cutoff **K = 10**:

| Mode | Recall@K | Precision@K | MRR@K | NDCG@K | Sep Ratio | Latency p50 | Latency p95 | Latency p99 | QPS |
|---|---|---|---|---|---|---|---|---|---|
| `bm25` | **1.000** | 0.100 | **0.250** | **0.431** | 1.83x | 4.64ms | 4.64ms | 4.64ms | **216** |
| `binary` | **0.000** | 0.000 | **0.000** | **0.000** | 1.08x | 0.11ms | 0.11ms | 0.11ms | **8865** |
| `ppr` | **1.000** | 0.100 | **0.500** | **0.631** | 41.58x | 4.83ms | 4.83ms | 4.83ms | **207** |
| `fast` | **1.000** | 0.100 | **0.500** | **0.631** | 1.24x | 4.81ms | 4.81ms | 4.81ms | **208** |

### Metric Descriptions
- **Recall@K**: Fraction of ground-truth relevant documents retrieved in the top K.
- **MRR@K**: Mean Reciprocal Rank (1/rank of first relevant result).
- **NDCG@K**: Normalized Discounted Cumulative Gain accounting for graded relevance.
- **Sep Ratio**: Score separation between top-1 hit and bottom top-K hit (confidence margin).
- **QPS**: Sustained queries per second.

