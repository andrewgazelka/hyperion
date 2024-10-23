# hyperion-stats

A high-performance parallel statistics library optimized for real-time anti-cheat systems, particularly in Minecraft servers. Uses SIMD operations to efficiently track multiple statistical metrics simultaneously.

## Overview

This library provides efficient parallel computation of running statistics (mean, variance, min/max) across multiple data streams simultaneously. This is particularly useful for anti-cheat systems that need to track many player metrics in real-time.

## Anti-Cheat Applications

Below are common anti-cheat metrics that can be tracked using parallel statistics:

| Metric | Description | Statistical Considerations |
|--------|-------------|---------------------------|
| Packets per Second | Track packet frequency per player | - Use sliding window<br>- Account for network jitter<br>- Track variance for burst detection |
| Position Delta | Distance between consecutive positions | - Consider tick rate<br>- Account for teleports<br>- Track max speed violations |
| Vertical Velocity | Changes in Y-coordinate | - Account for jump mechanics<br>- Consider block collisions<br>- Track unusual patterns |
| Click Patterns | Time between clicks | - Track click distribution<br>- Detect auto-clickers<br>- Consider CPS limits |
| Rotation Deltas | Changes in pitch/yaw | - Track smooth vs. snap movements<br>- Detect aimbot patterns<br>- Consider sensitivity |
| Block Interaction | Time between block breaks/places | - Account for tool efficiency<br>- Track unusual patterns<br>- Consider game mechanics |
| Combat Patterns | Hit timing and accuracy | - Track reach distances<br>- Consider ping/latency<br>- Detect impossible hits |
| Movement Timing | Time between movement packets | - Account for client tick rate<br>- Detect timer modifications<br>- Consider server load |

## Future Work & Considerations


### Additional Statistics
- Skewness and kurtosis for better pattern detection
- Exponential moving averages for trend detection
- Correlation between different metrics
- Fourier analysis for periodic pattern detection
- Entropy calculations for randomness assessment

### Performance Optimizations
- GPU acceleration for large player counts
- Adaptive sampling rates based on load
- Efficient memory management for long sessions
- Better SIMD utilization

### Anti-Cheat Specific Features
- Built-in violation level tracking
- Confidence scoring for detections
- False positive reduction algorithms
- Integration with common game mechanics
- Latency compensation

### Challenges to Consider
- Network conditions affecting measurements
- Server performance impact on timing
- Client-side modifications affecting data
- Game mechanic edge cases
- Balance between detection and false positives