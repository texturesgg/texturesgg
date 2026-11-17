# The launch video's app take

A scripted run of the app for the demo video: a player new to mods,
against the live textures.gg, its mods and ports in a scratch folder
(`$TMPDIR/tgg-studio-demo`). Background work runs inline in this scene,
so each step finishes before the next frame of the recording.

```sh
# ffmpeg on PATH (one that doesn't take the app's LD_LIBRARY_PATH)
target/release/tgg-studio --iso PATH --scene demo \
  --script crates/tgg-studio/demo/script.json \
  --video app.mp4 --duration 27 --fps 60 --size 1280x720 --scale 1.5 --pointer
```

Each run downloads tgg-melee and the Competitive collection for real, so
it adds an install to each of those mods' counts on textures.gg.
