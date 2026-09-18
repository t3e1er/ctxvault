# ctxvault Retrieval & Indexing Benchmark Report

- **Benchmark**: `repobench`
- **Repository**: `Meituan-AutoML/MobileVLM`
- **Evaluated Queries**: 1
- **Evaluation Cutoff**: K = 10

## 2. Retrieval Algorithm Quality & Latency Ablation

All IR metrics computed at cutoff **K = 10**:

| Mode | Recall@K | Precision@K | MRR@K | NDCG@K | Sep Ratio | Latency p50 | Latency p95 | Latency p99 | QPS |
|---|---|---|---|---|---|---|---|---|---|
| `bm25` | **1.000** | 0.100 | **1.000** | **1.000** | 1.90x | 10.75ms | 10.75ms | 10.75ms | **93** |
| `binary` | **1.000** | 0.100 | **0.250** | **0.431** | 1.19x | 1.66ms | 1.66ms | 1.66ms | **602** |
| `ppr` | **1.000** | 0.100 | **1.000** | **1.000** | 20.05x | 10.63ms | 10.63ms | 10.63ms | **94** |
| `fast` | **1.000** | 0.100 | **1.000** | **1.000** | 1.81x | 16.86ms | 16.86ms | 16.86ms | **59** |

### Metric Descriptions
- **Recall@K**: Fraction of ground-truth relevant documents retrieved in the top K.
- **MRR@K**: Mean Reciprocal Rank (1/rank of first relevant result).
- **NDCG@K**: Normalized Discounted Cumulative Gain accounting for graded relevance.
- **Sep Ratio**: Score separation between top-1 hit and bottom top-K hit (confidence margin).
- **QPS**: Sustained queries per second.

