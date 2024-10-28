import matplotlib.pyplot as plt

# Data in milliseconds
players = [1, 10, 100, 1000, 5000]
tick_time = [0.24, 0.30, 0.46, 0.40, 1.42]  # in ms

# Set figure size and DPI for high-resolution output suitable for GitHub
plt.figure(figsize=(12, 7), dpi=150)

# Create the plot
plt.plot(
    players, tick_time,
    marker='o',
    linestyle='-',
    color='#1f77b4',  # A pleasant blue color
    linewidth=2,
    markersize=8,
    label='Measured Tick Time',
    zorder=5
)

# Add 50ms threshold line
threshold = 50
plt.axhline(
    y=threshold,
    color='#d62728',  # A distinct red color
    linestyle='--',
    linewidth=2,
    label='50ms Threshold',
    zorder=4
)

# Highlight the area under the threshold
plt.fill_between(
    players,
    threshold,
    max(tick_time)*1.2,
    color='#ff7f0e',
    alpha=0.1,
    label='Under Threshold',
    zorder=1
)

# Customize x-axis to logarithmic scale for better representation of player counts
plt.xscale('log')

# **Set Custom X-Ticks Including 5000**
custom_ticks = [1, 10, 100, 1000, 5000]
plt.xticks(custom_ticks, [f'{int(x):,}' for x in custom_ticks])

# Set axis labels and title
plt.xlabel(
    'Number of Players',
    fontsize=12,
    fontweight='bold',
    labelpad=10
)
plt.ylabel(
    'Tick Time (ms)',
    fontsize=12,
    fontweight='bold',
    labelpad=10
)
plt.title(
    'Server Performance Analysis: Tick Time vs Player Count',
    fontsize=16,
    fontweight='bold',
    pad=20
)

# Set x and y limits appropriately
plt.xlim(1, max(players)*1.1)
plt.ylim(0, threshold + 10)  # Slightly above threshold for spacing

# Customize grid
plt.grid(True, which="both", linestyle='--', linewidth=0.5, alpha=0.7)

# Add legend with enhanced styling
plt.legend(
    frameon=True,
    fancybox=True,
    shadow=True,
    fontsize=10,
    loc='upper left'
)

# Add threshold annotation
plt.text(
    max(players)*0.05,
    threshold + 2,
    '50ms Tick Limit',
    color='#d62728',
    fontweight='bold',
    fontsize=10
)

# Add data point annotations
for x, y in zip(players, tick_time):
    plt.annotate(
        f'{y:.2f} ms',
        (x, y),
        textcoords="offset points",
        xytext=(0,10),
        ha='center',
        fontsize=9,
        color='#1f77b4'
    )

# **Ensure 5000 is Included in the X-Axis Tick Labels**
# (Already handled by setting custom_ticks above)

# Add explanatory text
plt.figtext(
    0.99, 0.01,
    'Note: Values below 50ms indicate optimal server performance.\n'
    'Higher values may result in server lag.',
    fontsize=9,
    color='#555555',
    ha='right',
    va='bottom'
)

# Ensure tight layout for better spacing
plt.tight_layout()

# Save as high-resolution PNG suitable for GitHub
plt.savefig(
    'performance.png',
    dpi=150,
    bbox_inches='tight',
    format='png'
)

plt.close()
