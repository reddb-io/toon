# Retrieval Accuracy Benchmark

Command: `pnpm benchmark:accuracy`

Provider: `openai`
Model: `gpt-4.1-mini`
Score: 6/8

| Format | Question | Expected | Answer | Result |
| --- | --- | --- | --- | --- |
| json | count-express | 10 | ```json<br>9<br>``` | fail |
| json | first-destination | AMS | {"destination":"AMS"} | pass |
| toon | count-express | 10 | toon  <br>10 | pass |
| toon | first-destination | AMS | toon<br><br>AMS | pass |
| toonExt | count-express | 10 | toonExt<br>9 | fail |
| toonExt | first-destination | AMS | toonExt: AMS | pass |
| toonl | count-express | 10 | toonl<br>10 | pass |
| toonl | first-destination | AMS | AMS | pass |

