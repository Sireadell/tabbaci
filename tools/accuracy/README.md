# Accuracy test

200 sentences (100 true, 100 false): "The capital of X is Y." Truth comes from Wikidata. The judges see a 300-character window of the country's Wikipedia summary, cut by the same code the courtroom uses, inside the same fixed frame.

```
node build-dataset.mjs                              rebuild dataset.json
node run.mjs --judge standin                        plumbing check with a fake judge (always scores about 100%, proves nothing about accuracy)
node run.mjs --judge near --key-file <file>         real judges on NEAR AI Cloud
```

The number that matters is "wrong": a ruling of true on a false sentence, or false on a true one. "No ruling" (judges unsure or split) costs nothing but speed.
