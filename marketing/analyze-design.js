const { chromium } = require('playwright');

async function analyzeDesign() {
  const browser = await chromium.launch();
  const page = await browser.newPage();

  await page.goto('https://ampcode.com/', { waitUntil: 'networkidle' });

  // Extract design tokens and styles
  const styles = await page.evaluate(() => {
    const results = {
      colors: new Set(),
      fonts: new Set(),
      elements: {}
    };

    // Get computed styles for key elements
    const elementsToAnalyze = [
      { selector: 'body', name: 'body' },
      { selector: 'h1', name: 'h1' },
      { selector: 'h2', name: 'h2' },
      { selector: 'p', name: 'p' },
      { selector: 'a', name: 'link' },
      { selector: 'button, [class*="button"], [class*="btn"]', name: 'button' },
      { selector: 'nav, header', name: 'nav' },
      { selector: '[class*="hero"], [class*="Hero"], section:first-of-type', name: 'hero' },
      { selector: '[class*="card"], [class*="Card"]', name: 'card' },
    ];

    elementsToAnalyze.forEach(({ selector, name }) => {
      const el = document.querySelector(selector);
      if (el) {
        const computed = window.getComputedStyle(el);
        results.elements[name] = {
          fontFamily: computed.fontFamily,
          fontSize: computed.fontSize,
          fontWeight: computed.fontWeight,
          lineHeight: computed.lineHeight,
          color: computed.color,
          backgroundColor: computed.backgroundColor,
          padding: computed.padding,
          margin: computed.margin,
          borderRadius: computed.borderRadius,
          letterSpacing: computed.letterSpacing,
        };
        results.colors.add(computed.color);
        results.colors.add(computed.backgroundColor);
        results.fonts.add(computed.fontFamily);
      }
    });

    // Get all CSS custom properties (variables)
    const root = document.documentElement;
    const rootStyles = window.getComputedStyle(root);
    results.cssVariables = {};

    // Try to get CSS variables from stylesheets
    for (const sheet of document.styleSheets) {
      try {
        for (const rule of sheet.cssRules) {
          if (rule.selectorText === ':root' || rule.selectorText === 'html') {
            const text = rule.cssText;
            const varMatches = text.match(/--[\w-]+:\s*[^;]+/g);
            if (varMatches) {
              varMatches.forEach(match => {
                const [name, value] = match.split(':').map(s => s.trim());
                results.cssVariables[name] = value;
              });
            }
          }
        }
      } catch (e) {
        // Cross-origin stylesheet, skip
      }
    }

    results.colors = [...results.colors];
    results.fonts = [...results.fonts];

    // Get page structure
    results.structure = {
      hasNav: !!document.querySelector('nav, header'),
      hasHero: !!document.querySelector('[class*="hero"], [class*="Hero"]'),
      hasSections: document.querySelectorAll('section').length,
      hasFooter: !!document.querySelector('footer'),
    };

    // Get specific color values from visible elements
    const allElements = document.querySelectorAll('*');
    const colorSet = new Set();
    allElements.forEach(el => {
      const style = window.getComputedStyle(el);
      if (style.color && style.color !== 'rgba(0, 0, 0, 0)') colorSet.add(style.color);
      if (style.backgroundColor && style.backgroundColor !== 'rgba(0, 0, 0, 0)') colorSet.add(style.backgroundColor);
    });
    results.allColors = [...colorSet].slice(0, 20);

    return results;
  });

  console.log('=== DESIGN ANALYSIS FOR AMPCODE.COM ===\n');
  console.log('CSS Variables:', JSON.stringify(styles.cssVariables, null, 2));
  console.log('\nFonts:', styles.fonts);
  console.log('\nColors found:', styles.allColors);
  console.log('\nElement styles:');
  Object.entries(styles.elements).forEach(([name, props]) => {
    console.log(`\n${name}:`);
    Object.entries(props).forEach(([prop, value]) => {
      if (value && value !== 'normal' && value !== 'none' && value !== '0px') {
        console.log(`  ${prop}: ${value}`);
      }
    });
  });
  console.log('\nPage structure:', styles.structure);

  // Take a screenshot
  await page.screenshot({ path: 'ampcode-screenshot.png', fullPage: true });
  console.log('\nScreenshot saved to ampcode-screenshot.png');

  // Get the HTML structure
  const html = await page.content();
  console.log('\n=== HTML STRUCTURE (first 3000 chars) ===\n');
  console.log(html.substring(0, 3000));

  await browser.close();
}

analyzeDesign().catch(console.error);
