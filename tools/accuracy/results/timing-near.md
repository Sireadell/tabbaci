# Real 3-judge timing

Judges asked one after another, as the courtroom does. Target 60 s, cap 90 s. Spent so far: $0.0008.

| Run | Sentence | Ruling | Total | Each judge (answer, signed receipt) |
|---|---|---|---|---|
| 1 | The capital of Afghanistan is Kabul. | T (truth T) | 4.6 s | DeepSeek-V4.1-Flash-1017: TRUE, 2.9 s, signed, matches; Qwen3.8-27B: TRUE, 0.8 s, signed, matches; Qwen3.6-35B-A3B-FP8: TRUE, 0.9 s, signed, matches |
| 2 | The capital of Albania is Tirana. | T (truth T) | 2.3 s | DeepSeek-V4.1-Flash-1017: TRUE, 0.8 s, signed, matches; Qwen3.8-27B: TRUE, 0.7 s, signed, matches; Qwen3.6-35B-A3B-FP8: TRUE, 0.8 s, signed, matches |
| 3 | The capital of Algeria is Algiers. | T (truth T) | 3.9 s | DeepSeek-V4.1-Flash-1017: TRUE, 0.8 s, signed, matches; Qwen3.8-27B: TRUE, 0.7 s, signed, matches; Qwen3.6-35B-A3B-FP8: TRUE, 2.4 s, signed, matches |

Slowest run 4.6 s: inside the 60 s target.
