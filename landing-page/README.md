# AgentDoctor Landing Page

Static website files for AgentDoctor.

## Structure

- `index.html`: lean landing page with install tabs and terminal preview.
- `docs/index.html`: concise documentation page.
- `privacy/index.html`: privacy policy.
- `terms/index.html`: terms of use.
- `assets/`: terminal screenshot, text-based social preview, CSS, and small
  JavaScript.
- `sitemap.xml` and `robots.txt`: crawler metadata.

The site uses folder `index.html` files so deployed URLs are clean:

```text
/
/docs/
/privacy/
/terms/
```

## Launch Notes

- Production domain: `https://agentdoctor.youssef.tn/`. Keep canonical tags,
  Open Graph tags, `sitemap.xml`, `robots.txt`, and `CNAME` in sync if it
  changes.
- Replace `assets/agentdoctor-terminal.png` if the terminal UI screenshot
  changes. Keep width and height attributes in sync to avoid layout shift.
- No template code, template assets, vendor fonts, or template license files are
  included in this implementation.
