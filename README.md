# Hyperion

![2024-07-11_15 37 33](https://github.com/user-attachments/assets/1d058da7-52fa-49e1-9d1e-4c368f3d623f)


# Running

## Step 1: The proxy

Go to `hyperion-proxy` and install it with `cargo install --path .`

## Step 2: The event (development)
```bash
brew install just
just debug
```

# Local CI

```
just
```

# Development

## Recommendations

- Wurst client
  - great for debugging and also rejoining with running `just debug`. I usually have an AutoReconnect time of 0 seconds.
- Supermaven. great code completion.
