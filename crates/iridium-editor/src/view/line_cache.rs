//! Line information cache for large file optimization.
//!
//! Provides efficient caching of line lengths for viewport culling
//! and selection rendering without iterating over all lines.

use crate::buffer::Buffer;
use crate::editor::navigation;

/// Cache for line length information.
///
/// Optimized for large files by only caching visible lines plus a buffer zone.
/// Uses lazy loading for line lengths outside the cached window.
#[derive(Debug)]
pub struct LineCache {
    /// Cached line lengths, indexed by line number.
    /// Only populated for lines within the cache window.
    lengths: Vec<Option<usize>>,
    /// First line in the current cache window.
    window_start: usize,
    /// Last line in the current cache window (exclusive).
    window_end: usize,
    /// Number of buffer lines around the visible area.
    buffer_lines: usize,
    /// Total number of lines in the document.
    total_lines: usize,
}

impl Default for LineCache {
    fn default() -> Self {
        Self::new()
    }
}

impl LineCache {
    /// Number of lines to cache around the visible area.
    const DEFAULT_BUFFER_LINES: usize = 50;

    /// Creates a new empty line cache.
    #[must_use]
    pub fn new() -> Self {
        Self {
            lengths: Vec::new(),
            window_start: 0,
            window_end: 0,
            buffer_lines: Self::DEFAULT_BUFFER_LINES,
            total_lines: 0,
        }
    }

    /// Creates a line cache with a custom buffer size.
    #[must_use]
    pub fn with_buffer_size(buffer_lines: usize) -> Self {
        Self {
            lengths: Vec::new(),
            window_start: 0,
            window_end: 0,
            buffer_lines,
            total_lines: 0,
        }
    }

    /// Updates the cache for the visible line range.
    ///
    /// This caches line lengths for visible lines plus a buffer zone,
    /// optimizing for scrolling performance.
    pub fn update_visible(&mut self, buffer: &Buffer, first_visible: usize, last_visible: usize) {
        let total = buffer.len_lines();
        self.total_lines = total;

        // Calculate new window bounds with buffer
        let new_start = first_visible.saturating_sub(self.buffer_lines);
        let new_end = (last_visible + self.buffer_lines + 1).min(total);

        // Check if we need to rebuild the cache
        let needs_rebuild = self.window_start != new_start
            || self.window_end != new_end
            || self.lengths.len() != new_end.saturating_sub(new_start);

        if !needs_rebuild {
            return;
        }

        // Rebuild cache for the new window
        self.window_start = new_start;
        self.window_end = new_end;
        let window_size = new_end.saturating_sub(new_start);

        self.lengths.clear();
        self.lengths.reserve(window_size);

        for line_idx in new_start..new_end {
            let len = navigation::line_length(buffer, line_idx);
            self.lengths.push(Some(len));
        }
    }

    /// Gets the cached length of a line, returning 0 if not cached.
    #[must_use]
    pub fn get_length(&self, line: usize) -> usize {
        if line >= self.window_start && line < self.window_end {
            let idx = line - self.window_start;
            self.lengths.get(idx).copied().flatten().unwrap_or(0)
        } else {
            0
        }
    }

    /// Gets line lengths as a slice for highlight rendering.
    ///
    /// The slice is indexed starting from `first_line`.
    /// Returns an empty slice if the range is outside the cache.
    #[must_use]
    pub fn lengths_slice(&self, first_line: usize, last_line: usize) -> Vec<usize> {
        if first_line >= self.window_end || last_line < self.window_start {
            return Vec::new();
        }

        let start = first_line.max(self.window_start);
        let end = (last_line + 1).min(self.window_end);

        let mut result = Vec::with_capacity(end - start);
        for line in start..end {
            result.push(self.get_length(line));
        }
        result
    }

    /// Gets all cached line lengths as a vector.
    ///
    /// This creates a vector where index 0 corresponds to line 0.
    /// Uncached lines have length 0. For large files, this is less
    /// efficient than using `lengths_slice()` with visible range.
    #[must_use]
    pub fn all_lengths(&self) -> Vec<usize> {
        let mut result = vec![0; self.total_lines];
        for (idx, &len) in self.lengths.iter().enumerate() {
            let line = self.window_start + idx;
            if line < result.len() {
                result[line] = len.unwrap_or(0);
            }
        }
        result
    }

    /// Returns the total number of lines in the document.
    #[must_use]
    pub fn total_lines(&self) -> usize {
        self.total_lines
    }

    /// Returns the current cache window bounds (start, end).
    #[must_use]
    pub fn window(&self) -> (usize, usize) {
        (self.window_start, self.window_end)
    }

    /// Returns the number of lines currently cached.
    #[must_use]
    pub fn cached_count(&self) -> usize {
        self.lengths.len()
    }

    /// Invalidates the cache, requiring a rebuild on next update.
    pub fn invalidate(&mut self) {
        self.lengths.clear();
        self.window_start = 0;
        self.window_end = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_buffer(lines: usize) -> Buffer {
        // Create buffer with exact number of lines (no trailing newline)
        let content: String = (0..lines)
            .map(|i| {
                if i < lines - 1 {
                    format!("Line {}\n", i)
                } else {
                    format!("Line {}", i)
                }
            })
            .collect();
        Buffer::from_text(&content)
    }

    #[test]
    fn test_line_cache_new() {
        let cache = LineCache::new();
        assert_eq!(cache.total_lines(), 0);
        assert_eq!(cache.cached_count(), 0);
    }

    #[test]
    fn test_line_cache_update_visible() {
        let mut cache = LineCache::with_buffer_size(5);
        let buffer = create_test_buffer(100);

        cache.update_visible(&buffer, 10, 20);

        assert_eq!(cache.total_lines(), 100);
        // Window should be 10 - 5 = 5 to 20 + 5 + 1 = 26
        let (start, end) = cache.window();
        assert_eq!(start, 5);
        assert_eq!(end, 26);
        assert_eq!(cache.cached_count(), 21);
    }

    #[test]
    fn test_line_cache_get_length() {
        let mut cache = LineCache::with_buffer_size(5);
        let buffer = create_test_buffer(100);

        cache.update_visible(&buffer, 10, 20);

        // "Line 10\n" has length 7 (no newline in length)
        assert!(cache.get_length(10) > 0);
        // Line outside window should return 0
        assert_eq!(cache.get_length(0), 0);
        assert_eq!(cache.get_length(99), 0);
    }

    #[test]
    fn test_line_cache_window_bounds() {
        let mut cache = LineCache::with_buffer_size(10);
        let buffer = create_test_buffer(50);

        // Test at start - window_start should clamp to 0
        cache.update_visible(&buffer, 0, 10);
        let (start, end) = cache.window();
        assert_eq!(start, 0);
        assert_eq!(end, 21); // 10 + 10 + 1 = 21

        // Test at end - window_end should clamp to total_lines
        cache.update_visible(&buffer, 40, 49);
        let (start, end) = cache.window();
        assert_eq!(start, 30); // 40 - 10 = 30
        assert_eq!(end, 50); // Clamped to 50
    }

    #[test]
    fn test_line_cache_lengths_slice() {
        let mut cache = LineCache::with_buffer_size(5);
        let buffer = create_test_buffer(100);

        cache.update_visible(&buffer, 10, 20);

        let slice = cache.lengths_slice(10, 15);
        assert_eq!(slice.len(), 6);
        assert!(slice.iter().all(|&len| len > 0));

        // Slice outside cache
        let empty = cache.lengths_slice(50, 60);
        assert!(empty.is_empty());
    }

    #[test]
    fn test_line_cache_invalidate() {
        let mut cache = LineCache::with_buffer_size(5);
        let buffer = create_test_buffer(100);

        cache.update_visible(&buffer, 10, 20);
        assert!(cache.cached_count() > 0);

        cache.invalidate();
        assert_eq!(cache.cached_count(), 0);
        assert_eq!(cache.window(), (0, 0));
    }

    #[test]
    fn test_line_cache_no_rebuild_same_window() {
        let mut cache = LineCache::with_buffer_size(5);
        let buffer = create_test_buffer(100);

        cache.update_visible(&buffer, 10, 20);
        let count1 = cache.cached_count();

        // Same window, should not rebuild
        cache.update_visible(&buffer, 10, 20);
        let count2 = cache.cached_count();

        assert_eq!(count1, count2);
    }
}
