use crate::platforms::AccessibilityEngine;
use crate::{AutomationError, Selector, UIElement, UIElementAttributes};
use std::sync::Arc;
use std::time::{Duration, Instant};

use super::ClickResult;

// Default timeout if none is specified on the locator itself
const DEFAULT_LOCATOR_TIMEOUT: Duration = Duration::from_secs(30);

/// A high-level API for finding and interacting with UI elements
#[derive(Clone)]
pub struct Locator {
    engine: Arc<dyn AccessibilityEngine>,
    selector: Selector,
    timeout: Duration, // Default timeout for this locator instance
    root: Option<UIElement>,
}

impl Locator {
    /// Create a new locator with the given selector
    pub(crate) fn new(engine: Arc<dyn AccessibilityEngine>, selector: Selector) -> Self {
        Self {
            engine,
            selector,
            timeout: DEFAULT_LOCATOR_TIMEOUT, // Use default
            root: None,
        }
    }

    /// Set a default timeout for waiting operations on this locator instance.
    /// This timeout is used if no specific timeout is passed to action/wait methods.
    pub fn set_default_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Set the root element for this locator
    pub fn within(mut self, element: UIElement) -> Self {
        self.root = Some(element);
        self
    }

    /// Get all elements matching this locator, waiting up to the specified timeout.
    /// If no timeout is provided, uses the locator's default timeout.
    pub async fn all(&self, timeout: Option<Duration>) -> Result<Vec<UIElement>, AutomationError> {
        let effective_timeout = timeout.unwrap_or(self.timeout);
        // find_elements itself handles the timeout now
        self.engine
            .find_elements(&self.selector, self.root.as_ref(), Some(effective_timeout))
    }

    /// Wait for an element matching the locator to appear, up to the specified timeout.
    /// If no timeout is provided, uses the locator's default timeout.
    pub async fn wait(&self, timeout: Option<Duration>) -> Result<UIElement, AutomationError> {
        let effective_timeout = timeout.unwrap_or(self.timeout);
        let start = std::time::Instant::now();

        loop {
            // Directly use find_element with the timeout
            match self.engine.find_element(
                &self.selector,
                self.root.as_ref(),
                Some(effective_timeout - start.elapsed()), // Pass remaining time
            ) {
                Ok(element) => return Ok(element),
                Err(AutomationError::ElementNotFound(_)) => {
                    // Continue looping if not found yet
                    if start.elapsed() >= effective_timeout {
                         // Use the original error message format if possible, or create a new one
                         return Err(AutomationError::Timeout(format!(
                            "Timed out after {:?} waiting for element {:?}",
                            effective_timeout, self.selector
                        )));
                    }
                    tokio::time::sleep(Duration::from_millis(100)).await; // Small delay before retry
                }
                 // Propagate other errors immediately
                 Err(e) => return Err(e),
            }
            // Redundant check, loop condition handles timeout
            // if start.elapsed() >= effective_timeout { ... }
        }
    }

    /// Get a nested locator
    pub fn locator(&self, selector: impl Into<Selector>) -> Locator {
        let next_selector = selector.into();
        let new_chain = match self.selector.clone() {
             // If the current selector is already a chain, append to it
             Selector::Chain(mut existing_chain) => {
                 existing_chain.push(next_selector);
                 existing_chain
             }
             // If the current selector is not a chain, create a new chain
             current_selector => {
                 vec![current_selector, next_selector]
             }
         };

        Locator {
            engine: self.engine.clone(),
            selector: Selector::Chain(new_chain), // Create the chain variant
            timeout: self.timeout, // Inherit timeout
            root: self.root.clone(), // Inherit root
        }
    }

    // --- Convenience methods for common actions ---
    // These now accept an optional timeout

    /// Click on the first matching element, waiting up to the specified timeout.
    /// If no timeout is provided, uses the locator's default timeout.
    pub async fn click(&self, timeout: Option<Duration>) -> Result<ClickResult, AutomationError> {
        let element = self.wait(timeout).await?;
        element.click()
    }

    /// Type text into the first matching element, waiting up to the specified timeout.
    /// If no timeout is provided, uses the locator's default timeout.
    pub async fn type_text(&self, text: &str, timeout: Option<Duration>) -> Result<(), AutomationError> {
        let element = self.wait(timeout).await?;
        element.type_text(text)
    }

    /// Press a key on the first matching element, waiting up to the specified timeout.
    /// If no timeout is provided, uses the locator's default timeout.
    pub async fn press_key(&self, key: &str, timeout: Option<Duration>) -> Result<(), AutomationError> {
        let element = self.wait(timeout).await?;
        element.press_key(key)
    }

    /// Get text from the first matching element, waiting up to the specified timeout.
    /// If no timeout is provided, uses the locator's default timeout.
    pub async fn text(&self, max_depth: usize, timeout: Option<Duration>) -> Result<String, AutomationError> {
        let element = self.wait(timeout).await?;
        element.text(max_depth)
    }

    /// Get attributes from the first matching element, waiting up to the specified timeout.
    /// If no timeout is provided, uses the locator's default timeout.
    pub async fn attributes(&self, timeout: Option<Duration>) -> Result<UIElementAttributes, AutomationError> {
        let element = self.wait(timeout).await?;
        Ok(element.attributes()) // attributes() itself doesn't return Result
    }

    /// Get bounds from the first matching element, waiting up to the specified timeout.
    /// If no timeout is provided, uses the locator's default timeout.
    pub async fn bounds(&self, timeout: Option<Duration>) -> Result<(f64, f64, f64, f64), AutomationError> {
        let element = self.wait(timeout).await?;
        element.bounds()
    }

    /// Check if the element is visible, waiting up to the specified timeout.
    /// If no timeout is provided, uses the locator's default timeout.
    pub async fn is_visible(&self, timeout: Option<Duration>) -> Result<bool, AutomationError> {
        // Wait might return ElementNotFound or Timeout, handle appropriately
        match self.wait(timeout).await {
            Ok(element) => element.is_visible(),
            Err(AutomationError::Timeout(_)) | Err(AutomationError::ElementNotFound(_)) => {
                 // If the element wasn't found within the timeout, it's not visible
                 Ok(false)
            }
            Err(e) => Err(e), // Propagate other errors
        }
    }

    // --- Expectation Methods ---
    // These already handle timeouts internally via loops

    /// Waits for the element matched by the locator to be enabled.
    pub async fn expect_enabled(&self, timeout: Option<Duration>) -> Result<UIElement, AutomationError> {
        let effective_timeout = timeout.unwrap_or(self.timeout);
        let start = Instant::now();

        loop {
            // Use self.wait with a short internal timeout for each check? Or direct find_element?
            // Using find_element directly within the loop is more efficient here.
            match self.engine.find_element(&self.selector, self.root.as_ref(), Some(Duration::from_millis(100))) { // Short timeout for check
                Ok(element) => {
                    match element.is_enabled() {
                        Ok(true) => return Ok(element),
                        Ok(false) => { /* Condition not met, continue loop */ }
                        Err(e) => { /* Error checking enabled state, maybe retry or fail */
                             if !matches!(e, AutomationError::ElementNotFound(_)) { // Ignore not found during check
                                return Err(e); // Return other errors from is_enabled
                             }
                        }
                    }
                }
                Err(AutomationError::ElementNotFound(_)) => { /* Element not found yet, continue loop */ }
                Err(e) => return Err(e), // Error finding the element
            }

            if start.elapsed() >= effective_timeout {
                return Err(AutomationError::Timeout(format!(
                    "Timed out after {:?} waiting for element {:?} to be enabled",
                    effective_timeout, self.selector
                )));
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }

    /// Waits for the element matched by the locator to be visible.
    pub async fn expect_visible(&self, timeout: Option<Duration>) -> Result<UIElement, AutomationError> {
        let effective_timeout = timeout.unwrap_or(self.timeout);
        let start = Instant::now();

        loop {
             // Use find_element directly
             match self.engine.find_element(&self.selector, self.root.as_ref(), Some(Duration::from_millis(100))) {
                Ok(element) => {
                    match element.is_visible() {
                        Ok(true) => return Ok(element),
                        Ok(false) => { /* Condition not met, continue loop */ }
                        Err(e) => {
                             if !matches!(e, AutomationError::ElementNotFound(_)) {
                                return Err(e);
                             }
                        }
                    }
                }
                Err(AutomationError::ElementNotFound(_)) => { /* Element not found yet, continue loop */ }
                Err(e) => return Err(e),
            }

            if start.elapsed() >= effective_timeout {
                return Err(AutomationError::Timeout(format!(
                    "Timed out after {:?} waiting for element {:?} to be visible",
                    effective_timeout, self.selector
                )));
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }

    /// Waits for the element's text content to match the expected text.
    pub async fn expect_text_equals(&self, expected_text: &str, max_depth: usize, timeout: Option<Duration>) -> Result<UIElement, AutomationError> {
        let effective_timeout = timeout.unwrap_or(self.timeout);
        let start = Instant::now();

        loop {
             // Use find_element directly
             match self.engine.find_element(&self.selector, self.root.as_ref(), Some(Duration::from_millis(100))) {
                Ok(element) => {
                    match element.text(max_depth) {
                        // Trim both actual and expected for comparison robustness
                        Ok(actual_text) if actual_text.trim() == expected_text.trim() => return Ok(element),
                        Ok(_) => { /* Text doesn't match, continue loop */ }
                        Err(e) => {
                             if !matches!(e, AutomationError::ElementNotFound(_)) {
                                return Err(e);
                             }
                        }
                    }
                }
                Err(AutomationError::ElementNotFound(_)) => { /* Element not found yet, continue loop */ }
                Err(e) => return Err(e),
            }

            if start.elapsed() >= effective_timeout {
                 // Include actual text in timeout message if possible? Might require getting text one last time.
                return Err(AutomationError::Timeout(format!(
                    "Timed out after {:?} waiting for element {:?} text to equal '{}'",
                    effective_timeout, self.selector, expected_text
                )));
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }
}
