# ctxvault Retrieval & Indexing Benchmark Report

- **Benchmark**: `codesearchnet`
- **Repository**: `gin-gonic/gin`
- **Evaluated Queries**: 0
- **Evaluation Cutoff**: K = 10

## 2. Retrieval Algorithm Quality & Latency Ablation

All IR metrics computed at cutoff **K = 10**:

| Mode | Recall@K | Precision@K | MRR@K | NDCG@K | Sep Ratio | Latency p50 | Latency p95 | Latency p99 | QPS |
|---|---|---|---|---|---|---|---|---|---|
| `bm25` | **0.000** | 0.000 | **0.000** | **0.000** | 0.00x | 0.00ms | 0.00ms | 0.00ms | **0** |
| `binary` | **0.000** | 0.000 | **0.000** | **0.000** | 0.00x | 0.00ms | 0.00ms | 0.00ms | **0** |
| `ppr` | **0.000** | 0.000 | **0.000** | **0.000** | 0.00x | 0.00ms | 0.00ms | 0.00ms | **0** |
| `fast` | **0.000** | 0.000 | **0.000** | **0.000** | 0.00x | 0.00ms | 0.00ms | 0.00ms | **0** |

### Metric Descriptions
- **Recall@K**: Fraction of ground-truth relevant documents retrieved in the top K.
- **MRR@K**: Mean Reciprocal Rank (1/rank of first relevant result).
- **NDCG@K**: Normalized Discounted Cumulative Gain accounting for graded relevance.
- **Sep Ratio**: Score separation between top-1 hit and bottom top-K hit (confidence margin).
- **QPS**: Sustained queries per second.

