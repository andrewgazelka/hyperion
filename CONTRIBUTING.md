# Contributing to Hyperion
We love pull requests! If you consistently contribute quality PRs, @andrewgazelka will sponsor your subscriptions to AI development tools to enhance your productivity.

## Recommended Development Setup

### IDEs & Tools
- **IntelliJ IDEA** - Primary recommended IDE
- [Cursor](https://cursor.com) - AI-powered IDE
    - [cursor-sync](https://github.com/andrewgazelka/cursor-sync) - Plugin to sync cursor position between Cursor and IntelliJ
- [Supermaven](https://www.supermaven.com) - For enhanced code completion

### Code Quality Tools
- **pre-commit** - Ensures code quality before commits
    1. Install pre-commit: `pip install pre-commit`
    2. Install the hooks (optional): `pre-commit install`
    3. Run against all files (optional): `pre-commit run --all-files`

The pre-commit configuration will automatically:
- Format Rust code with `rustfmt`
- Run other code quality checks

Pre-commit hooks run automatically on `git commit`. If any checks fail, fix the issues and try committing again.

### Helpful Development Tools
- [Packet Inspector](https://github.com/valence-rs/valence/tree/main/tools/packet_inspector) - Useful for debugging Minecraft protocol packets
- [flecs.dev](https://flecs.dev/explorer) super useful for viewing entities
- tracy

![image](https://github.com/user-attachments/assets/51f99c9a-a535-4fd8-9a9f-12f8e3039d04)


### Protocol Documentation
We use Minecraft 1.20.1 protocol documentation:
- Reference: [Wiki.vg Protocol (Version 18375)](https://wiki.vg/index.php?title=Protocol&oldid=18375)

### Working with Protocol Documentation and LLMs

To effectively work with LLMs for packet analysis:

1. Get Wiki.vg Documentation:
    - Install [MarkDownload](https://chromewebstore.google.com/detail/markdownload-markdown-web/pcmpcfapbekmbjjkdalcgopdkipoggdi) browser extension
    - Navigate to [Wiki.vg Protocol](https://wiki.vg/index.php?title=Protocol&oldid=18375)
    - Use MarkDownload to save the page as Markdown
    - This gives you the protocol specification in a format you can share with LLMs

2. Include Valence Reference:
    - Share Valence's [packets.json](https://github.com/valence-rs/valence/blob/8f3f84d557dacddd7faddb2ad724185ecee2e482/tools/packet_inspector/extracted/packets.json) with the LLM
    - This helps the LLM understand packet name mappings between Wiki.vg and Valence

3. Ask Questions:
    - Provide both the Wiki.vg markdown and relevant parts of packets.json
    - LLMs can then help map between Wiki.vg and Valence packet structures
    - Get assistance with implementation details and packet field mappings

## Getting Help
If you're stuck or have questions:
- Open an issue for discussion
- Join our [Discord](https://discord.gg/PBfnDtj5Wb) for real-time help
- Check existing PRs and issues for similar discussions

## Pull Request Process
1. Fork the repository
2. Create a feature branch
3. Commit your changes
4. Push to your fork
5. Open a Pull Request

We aim to review PRs promptly and provide constructive feedback when needed.

Thank you for contributing to Hyperion!
