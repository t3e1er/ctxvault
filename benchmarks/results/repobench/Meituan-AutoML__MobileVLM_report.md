# ctxvault Retrieval & Indexing Benchmark Report

- **Benchmark**: `repobench`
- **Repository**: `Meituan-AutoML/MobileVLM`
- **Evaluated Queries**: 1
- **Evaluation Cutoff**: K = 10

## 2. Retrieval Algorithm Quality & Latency Ablation

All IR metrics computed at cutoff **K = 10**:

| Mode | Recall@K | Precision@K | MRR@K | NDCG@K | Sep Ratio | Latency p50 | Latency p95 | Latency p99 | QPS |
|---|---|---|---|---|---|---|---|---|---|
| `bm25` | **1.000** | 0.100 | **1.000** | **1.000** | 1.90x | 19.31ms | 19.31ms | 19.31ms | **52** |
| `binary` | **0.000** | 0.000 | **0.000** | **0.000** | 1.09x | 1.32ms | 1.32ms | 1.32ms | **760** |
| `ppr` | **1.000** | 0.100 | **1.000** | **1.000** | 20.05x | 20.14ms | 20.14ms | 20.14ms | **50** |
| `fast` | **1.000** | 0.100 | **1.000** | **1.000** | 1.23x | 28.98ms | 28.98ms | 28.98ms | **35** |

### Metric Descriptions
- **Recall@K**: Fraction of ground-truth relevant documents retrieved in the top K.
- **MRR@K**: Mean Reciprocal Rank (1/rank of first relevant result).
- **NDCG@K**: Normalized Discounted Cumulative Gain accounting for graded relevance.
- **Sep Ratio**: Score separation between top-1 hit and bottom top-K hit (confidence margin).
- **QPS**: Sustained queries per second.

