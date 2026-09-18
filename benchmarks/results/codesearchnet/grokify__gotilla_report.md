# ctxvault Retrieval & Indexing Benchmark Report

- **Benchmark**: `codesearchnet`
- **Repository**: `grokify/gotilla`
- **Evaluated Queries**: 2
- **Evaluation Cutoff**: K = 10

## 2. Retrieval Algorithm Quality & Latency Ablation

All IR metrics computed at cutoff **K = 10**:

| Mode | Recall@K | Precision@K | MRR@K | NDCG@K | Sep Ratio | Latency p50 | Latency p95 | Latency p99 | QPS |
|---|---|---|---|---|---|---|---|---|---|
| `bm25` | **0.750** | 0.100 | **0.250** | **0.331** | 1.99x | 40.41ms | 48.66ms | 49.40ms | **25** |
| `binary` | **0.250** | 0.050 | **0.062** | **0.097** | 1.09x | 0.95ms | 0.99ms | 0.99ms | **1051** |
| `ppr` | **0.750** | 0.100 | **0.550** | **0.589** | 11.72x | 39.53ms | 48.92ms | 49.75ms | **25** |
| `fast` | **0.750** | 0.100 | **0.350** | **0.387** | 1.71x | 46.44ms | 55.63ms | 56.45ms | **22** |

### Metric Descriptions
- **Recall@K**: Fraction of ground-truth relevant documents retrieved in the top K.
- **MRR@K**: Mean Reciprocal Rank (1/rank of first relevant result).
- **NDCG@K**: Normalized Discounted Cumulative Gain accounting for graded relevance.
- **Sep Ratio**: Score separation between top-1 hit and bottom top-K hit (confidence margin).
- **QPS**: Sustained queries per second.

