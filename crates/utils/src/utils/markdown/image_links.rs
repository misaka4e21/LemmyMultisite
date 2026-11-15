use super::link_rule::Link;
use crate::utils::markdown::link_rule;
use markdown_it::{
  MarkdownIt, NodeValue,
  parser::linkfmt::LinkFormatter,
  plugins::{
    cmark::{
      block::fence,
      inline::image::{self, Image},
    },
    extra::linkify::{self, Linkified},
  },
};
use std::sync::LazyLock;
use url::Url;
use urlencoding::encode;

/// Rewrites all links to remote domains in markdown, so they go through `/api/v4/image_proxy`.
pub fn markdown_rewrite_image_links(
  mut src: String,
  local_hostname: &str,
  protocol_and_hostname: &str,
) -> (String, Vec<Url>) {
  let links_offsets = find_urls::<Image>(&src);

  let mut links = vec![];
  // Go through the collected links in reverse order
  for (start, end) in links_offsets.into_iter().rev() {
    let (url, extra) = markdown_handle_title(&src, start, end);
    match Url::parse(url) {
      Ok(parsed) => {
        links.push(parsed.clone());
        // If link points to remote domain, replace with proxied link
        if parsed.domain() != Some(local_hostname) {
          let mut proxied = format!(
            "{}/api/v4/image/proxy?url={}",
            protocol_and_hostname,
            encode(url),
          );
          // restore custom emoji format
          if let Some(extra) = extra {
            proxied.push(' ');
            proxied.push_str(extra);
          }
          src.replace_range(start..end, &proxied);
        }
      }
      Err(_) => {
        // If its not a valid url, replace with empty text
        src.replace_range(start..end, "");
      }
    }
  }

  (src, links)
}

pub fn markdown_handle_title(src: &str, start: usize, end: usize) -> (&str, Option<&str>) {
  let content = src.get(start..end).unwrap_or_default();
  // necessary for custom emojis which look like `![name](url "title")`
  match content.split_once(' ') {
    Some((a, b)) => (a, Some(b)),
    _ => (content, None),
  }
}

pub fn markdown_find_links(src: &str) -> Vec<(usize, usize)> {
  let mut res = vec![];
  res.extend(find_urls::<Linkified>(src));
  res.extend(find_urls::<Link>(src));
  res
}

/// Finds image URL ranges using the same Markdown parser as image rewriting.
pub fn markdown_find_image_links(src: &str) -> Vec<(usize, usize)> {
  find_urls::<Image>(src)
}

/// Resolves local image and proxy URLs without treating an external alias as local.
pub fn local_image_alias(url: &Url, site_origins: &[Url]) -> Option<String> {
  local_image_source(url, site_origins).map(|(alias, _)| alias)
}

/// Returns the alias and the origin which actually serves it, including through local proxies.
pub fn local_image_source(url: &Url, site_origins: &[Url]) -> Option<(String, String)> {
  let mut current = url.clone();
  for _ in 0..4 {
    if !site_origins
      .iter()
      .any(|site| site.origin() == current.origin())
    {
      return None;
    }
    if matches!(
      current.path(),
      "/api/v4/image/proxy" | "/api/v3/image_proxy"
    ) {
      let nested = current.query_pairs().find(|(key, _)| key == "url")?.1;
      current = Url::parse(&nested).ok()?;
      continue;
    }
    let path = current.path();
    let encoded = path
      .strip_prefix("/api/v4/image/")
      .or_else(|| path.strip_prefix("/pictrs/image/"))?;
    let alias = urlencoding::decode(encoded).ok()?;
    if alias.is_empty() || alias.contains(['/', '\\']) {
      return None;
    }
    return Some((alias.into_owned(), current.origin().ascii_serialization()));
  }
  None
}

/// Flags local-looking media URLs whose serving site or alias cannot be determined safely.
pub fn unresolved_local_media_url(raw: &str, site_origins: &[Url]) -> bool {
  let media_path = |path: &str| {
    path.starts_with("/api/v4/image/")
      || path.starts_with("/pictrs/image/")
      || path == "/api/v3/image_proxy"
  };
  let Ok(mut current) = Url::parse(raw) else {
    return media_path(raw);
  };
  for _ in 0..4 {
    let same_host = site_origins
      .iter()
      .any(|site| site.host_str() == current.host_str());
    if !same_host {
      return false;
    }
    let known_origin = site_origins
      .iter()
      .any(|site| site.origin() == current.origin());
    if !known_origin {
      return media_path(current.path());
    }
    if matches!(
      current.path(),
      "/api/v4/image/proxy" | "/api/v3/image_proxy"
    ) {
      let Some(nested) = current.query_pairs().find(|(key, _)| key == "url") else {
        return true;
      };
      let Ok(url) = Url::parse(&nested.1) else {
        return true;
      };
      current = url;
      continue;
    }
    return media_path(current.path()) && local_image_source(&current, site_origins).is_none();
  }
  true
}

/// Collects locally hosted media links from Markdown without fetching any URL.
pub fn markdown_local_media_aliases(src: &str, site_origins: &[Url]) -> Vec<String> {
  let mut aliases: Vec<String> = markdown_local_media_sources(src, site_origins)
    .into_iter()
    .map(|(alias, _)| alias)
    .collect();
  aliases.sort_unstable();
  aliases.dedup();
  aliases
}

pub fn markdown_local_media_sources(src: &str, site_origins: &[Url]) -> Vec<(String, String)> {
  let mut ranges = markdown_find_links(src);
  ranges.extend(markdown_find_image_links(src));
  let mut aliases: Vec<(String, String)> = ranges
    .into_iter()
    .filter_map(|(start, end)| Url::parse(markdown_handle_title(src, start, end).0).ok())
    .filter_map(|url| local_image_source(&url, site_origins))
    .collect();
  aliases.sort_unstable();
  aliases.dedup();
  aliases
}

// Walk the syntax tree to find positions of image or link urls
fn find_urls<T: NodeValue + UrlAndTitle>(src: &str) -> Vec<(usize, usize)> {
  // Use separate markdown parser here, with most features disabled for faster parsing,
  // and a dummy link formatter which doesnt normalize links.
  static PARSER: LazyLock<MarkdownIt> = LazyLock::new(|| {
    let mut p = MarkdownIt::new();
    p.link_formatter = Box::new(NoopLinkFormatter {});
    linkify::add(&mut p);
    image::add(&mut p);
    fence::add(&mut p);
    link_rule::add(&mut p);
    p
  });

  let ast = PARSER.parse(src);
  let mut links_offsets = vec![];
  ast.walk(|node, _depth| {
    if let Some(image) = node.cast::<T>()
      && let Some(srcmap) = node.srcmap
    {
      let (_, node_offset) = srcmap.get_byte_offsets();

      links_offsets.push((
        image.start_offset(node_offset),
        image.end_offset(node_offset),
      ));
    }
  });
  links_offsets
}

trait UrlAndTitle {
  fn start_offset(&self, node_offset: usize) -> usize;
  fn end_offset(&self, node_offset: usize) -> usize;
}

impl UrlAndTitle for Image {
  fn start_offset(&self, node_offset: usize) -> usize {
    start_offset_impl(&self.title, &self.url, node_offset)
  }
  fn end_offset(&self, node_offset: usize) -> usize {
    node_offset - 1
  }
}
impl UrlAndTitle for Link {
  fn start_offset(&self, node_offset: usize) -> usize {
    start_offset_impl(&self.title, &self.url, node_offset)
  }
  fn end_offset(&self, node_offset: usize) -> usize {
    node_offset - 1
  }
}

fn start_offset_impl(title: &Option<String>, url: &str, node_offset: usize) -> usize {
  let title_len = title.as_ref().map(|t| t.len() + 3).unwrap_or_default();
  node_offset - url.len() - 1 - title_len
}

impl UrlAndTitle for Linkified {
  fn start_offset(&self, node_offset: usize) -> usize {
    node_offset - self.url.len() + 1
  }
  fn end_offset(&self, node_offset: usize) -> usize {
    node_offset + 1
  }
}

/// markdown-it normalizes links by default, which breaks the link rewriting. So we use a dummy
/// formatter here which does nothing. Note this isnt actually used to render the markdown.
#[derive(Debug)]
struct NoopLinkFormatter;

impl LinkFormatter for NoopLinkFormatter {
  fn validate_link(&self, _url: &str) -> Option<()> {
    Some(())
  }

  fn normalize_link(&self, url: &str) -> String {
    url.to_owned()
  }

  fn normalize_link_text(&self, url: &str) -> String {
    url.to_owned()
  }
}

#[cfg(test)]
mod tests {

  use super::*;
  use pretty_assertions::assert_eq;

  #[test]
  fn test_find_links() {
    let links = markdown_find_links("[test](https://example.com)");
    assert_eq!(vec![(7, 26)], links);

    let links = markdown_find_image_links("![test](https://example.com)");
    assert_eq!(vec![(8, 27)], links);

    let links = find_urls::<Image>("![ითხოვს](https://example.com/ითხოვს)");
    assert_eq!(vec![(22, 60)], links);

    let links = find_urls::<Image>("![test](https://example.com/%C3%A4%C3%B6%C3%BC.jpg)");
    assert_eq!(vec![(8, 50)], links);
  }

  #[test]
  fn find_embedded_images_excludes_code_and_plain_links() {
    let markdown = "![photo](https://a.example/api/v4/image/alias.png \"caption\")\n\
      [link](https://a.example/api/v4/image/other.png)\n\
      ```\n![code](https://a.example/api/v4/image/code.png)\n```";
    let urls = markdown_find_image_links(markdown)
      .into_iter()
      .map(|(start, end)| markdown_handle_title(markdown, start, end).0)
      .collect::<Vec<_>>();
    assert_eq!(urls, vec!["https://a.example/api/v4/image/alias.png"]);
  }

  #[test]
  fn resolve_local_image_alias_requires_owned_origin() {
    let sites = [
      Url::parse("https://a.example").unwrap(),
      Url::parse("https://b.example").unwrap(),
    ];
    let resolve = |value| local_image_alias(&Url::parse(value).unwrap(), &sites);
    assert_eq!(
      resolve("https://a.example/api/v4/image/a.png"),
      Some("a.png".into())
    );
    assert_eq!(
      resolve("https://b.example/pictrs/image/a.png"),
      Some("a.png".into())
    );
    assert_eq!(
      resolve(
        "https://b.example/api/v4/image/proxy?url=https%3A%2F%2Fa.example%2Fapi%2Fv4%2Fimage%2Fa.png"
      ),
      Some("a.png".into())
    );
    assert_eq!(resolve("https://foreign.example/api/v4/image/a.png"), None);
    assert_eq!(
      resolve(
        "https://a.example/api/v4/image/proxy?url=https%3A%2F%2Fforeign.example%2Fapi%2Fv4%2Fimage%2Fa.png"
      ),
      None
    );
    assert_eq!(resolve("https://a.example/api/v4/image/a%2Fb.png"), None);
    assert!(unresolved_local_media_url(
      "http://a.example/api/v4/image/a.png",
      &sites
    ));
    assert!(unresolved_local_media_url("/api/v4/image/a.png", &sites));
    assert!(!unresolved_local_media_url(
      "https://foreign.example/api/v4/image/a.png",
      &sites
    ));
  }

  #[test]
  fn collect_local_media_from_markdown() {
    let sites = [Url::parse("https://a.example").unwrap()];
    let content = "![photo](https://a.example/api/v4/image/a.png) \
      [link](https://a.example/api/v4/image/a.png) \
      ![foreign](https://foreign.example/api/v4/image/a.png)";
    assert_eq!(markdown_local_media_aliases(content, &sites), vec!["a.png"]);
  }

  #[test]
  fn test_markdown_proxy_images() {
    let tests: Vec<_> = vec![
      (
        "remote image proxied",
        "![link](http://example.com/image.jpg)",
        "![link](https://lemmy-alpha/api/v4/image/proxy?url=http%3A%2F%2Fexample.com%2Fimage.jpg)",
      ),
      (
        "local image unproxied",
        "![link](http://lemmy-alpha/image.jpg)",
        "![link](http://lemmy-alpha/image.jpg)",
      ),
      (
        "multiple image links",
        "![link](http://example.com/image1.jpg) ![link](http://example.com/image2.jpg)",
        "![link](https://lemmy-alpha/api/v4/image/proxy?url=http%3A%2F%2Fexample.com%2Fimage1.jpg) ![link](https://lemmy-alpha/api/v4/image/proxy?url=http%3A%2F%2Fexample.com%2Fimage2.jpg)",
      ),
      ("empty link handled", "![image]()", "![image]()"),
      (
        "empty label handled",
        "![](http://example.com/image.jpg)",
        "![](https://lemmy-alpha/api/v4/image/proxy?url=http%3A%2F%2Fexample.com%2Fimage.jpg)",
      ),
      (
        "invalid image link removed",
        "![image](http-not-a-link)",
        "![image]()",
      ),
      (
        "label with nested markdown handled",
        "![a *b* c](http://example.com/image.jpg)",
        "![a *b* c](https://lemmy-alpha/api/v4/image/proxy?url=http%3A%2F%2Fexample.com%2Fimage.jpg)",
      ),
      (
        "custom emoji support",
        r#"![party-blob](https://www.hexbear.net/pictrs/image/83405746-0620-4728-9358-5f51b040ffee.gif "emoji party-blob")"#,
        r#"![party-blob](https://lemmy-alpha/api/v4/image/proxy?url=https%3A%2F%2Fwww.hexbear.net%2Fpictrs%2Fimage%2F83405746-0620-4728-9358-5f51b040ffee.gif "emoji party-blob")"#,
      ),
      (
        "image with special chars",
        "ითხოვს ![ითხოვს](http://example.com/ითხოვს%C3%A4.jpg)",
        "ითხოვს ![ითხოვს](https://lemmy-alpha/api/v4/image/proxy?url=http%3A%2F%2Fexample.com%2F%E1%83%98%E1%83%97%E1%83%AE%E1%83%9D%E1%83%95%E1%83%A1%25C3%25A4.jpg)",
      ),
    ];

    tests.iter().for_each(|&(msg, input, expected)| {
      let result =
        markdown_rewrite_image_links(input.to_string(), "lemmy-alpha", "https://lemmy-alpha");

      assert_eq!(
        result.0, expected,
        "Testing {}, with original input '{}'",
        msg, input
      );
    });
  }
}
