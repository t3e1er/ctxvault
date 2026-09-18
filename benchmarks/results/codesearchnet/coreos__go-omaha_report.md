# ctxvault Retrieval & Indexing Benchmark Report

- **Benchmark**: `codesearchnet`
- **Repository**: `coreos/go-omaha`
- **Evaluated Queries**: 3
- **Evaluation Cutoff**: K = 10

## 2. Retrieval Algorithm Quality & Latency Ablation

All IR metrics computed at cutoff **K = 10**:

| Mode | Recall@K | Precision@K | MRR@K | NDCG@K | Sep Ratio | Latency p50 | Latency p95 | Latency p99 | QPS |
|---|---|---|---|---|---|---|---|---|---|
| `bm25` | **1.000** | 0.100 | **1.000** | **1.000** | 1.75x | 6.88ms | 7.01ms | 7.03ms | **145** |
| `binary` | **1.000** | 0.100 | **0.200** | **0.387** | 1.25x | 0.42ms | 0.42ms | 0.42ms | **2398** |
| `ppr` | **1.000** | 0.100 | **1.000** | **1.000** | 45.75x | 5.15ms | 5.73ms | 5.78ms | **200** |
| `fast` | **1.000** | 0.100 | **1.000** | **1.000** | 3.28x | 11.35ms | 13.75ms | 13.96ms | **87** |

### Metric Descriptions
- **Recall@K**: Fraction of ground-truth relevant documents retrieved in the top K.
- **MRR@K**: Mean Reciprocal Rank (1/rank of first relevant result).
- **NDCG@K**: Normalized Discounted Cumulative Gain accounting for graded relevance.
- **Sep Ratio**: Score separation between top-1 hit and bottom top-K hit (confidence margin).
- **QPS**: Sustained queries per second.

