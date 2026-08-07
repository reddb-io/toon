# Accuracy Benchmark Report

## Offline verification (reproducible)

- Command: `pnpm benchmark:accuracy:verify`
- Suite: version 2, seed 218
- LLM access: not required

## Model observations (non-CI)

These model observations are reporting artifacts and are not merge-gate evidence.

| Task | Model | Format | Retries | Tokens | Syntax valid | Semantic accuracy | Raw artifacts |
| --- | --- | --- | ---: | ---: | --- | ---: | --- |
| release-roster | fixture-model | json | 0 | 80 | yes | 100.0% | raw/release-roster-json-attempt-1.txt |
| release-roster | fixture-model | toon | 0 | 75 | yes | 100.0% | raw/release-roster-toon-attempt-1.txt |
| deployment-window | fixture-model | json | 1 | 120 | yes | 100.0% | raw/deployment-window-json-attempt-1.txt<br>raw/deployment-window-json-attempt-2.txt |
| deployment-window | fixture-model | toon | 1 | 113 | yes | 100.0% | raw/deployment-window-toon-attempt-1.txt<br>raw/deployment-window-toon-attempt-2.txt |

