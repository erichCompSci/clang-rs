// Copyright 2016 Kyle Mayes
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Issues with source files.

use std::fmt;
use std::mem;

use clang_sys as ffi;

use utility;
use super::{TranslationUnit};
use super::source::{SourceLocation, SourceRange};

//================================================
// Enums
//================================================

// FixIt _________________________________________

/// A suggested fix for an issue with a source file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FixIt<'tu> {
    /// Delete a segment of the source file.
    Deletion(SourceRange<'tu>),
    /// Insert a string into the source file.
    Insertion(SourceLocation<'tu>, String),
    /// Replace a segment of the source file with a string.
    Replacement(SourceRange<'tu>, String),
}

// Severity ______________________________________

/// Indicates the severity of a diagnostic.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
#[repr(C)]
pub enum Severity {
    /// The diagnostic has been suppressed (e.g., by a command-line option).
    Ignored = 0,
    /// The diagnostic is attached to the previous non-note diagnostic.
    Note = 1,
    /// The diagnostic targets suspicious code that may or may not be wrong.
    Warning = 2,
    /// The diagnostic targets ill-formed code.
    Error = 3,
    /// The diagnostic targets code that is ill-formed in such a way that parser recovery is
    /// unlikely to produce any useful results.
    Fatal = 4,
}

//================================================
// Structs
//================================================

// Diagnostic ____________________________________

/// A message from the compiler about an issue with a source file.
#[derive(Copy, Clone)]
pub struct Diagnostic<'tu> {
    ptr: ffi::CXDiagnostic,
    tu: &'tu TranslationUnit<'tu>,
}

impl<'tu> Diagnostic<'tu> {
    //- Constructors -----------------------------

    #[doc(hidden)]
    pub fn from_ptr(ptr: ffi::CXDiagnostic, tu: &'tu TranslationUnit<'tu>) -> Diagnostic<'tu> {
        Diagnostic { ptr: ptr, tu: tu }
    }

    //- Accessors --------------------------------

    /// Returns the severity of this diagnostic.
    pub fn get_severity(&self) -> Severity {
        unsafe { mem::transmute(ffi::clang_getDiagnosticSeverity(self.ptr)) }
    }

    /// Returns the text of this diagnostic.
    pub fn get_text(&self) -> String {
        unsafe { utility::to_string(ffi::clang_getDiagnosticSpelling(self.ptr)) }
    }

    /// Returns the source location of this diagnostic.
    pub fn get_location(&self) -> SourceLocation<'tu> {
        unsafe { SourceLocation::from_raw(ffi::clang_getDiagnosticLocation(self.ptr), self.tu) }
    }

    /// Returns the source ranges of this diagnostic.
    pub fn get_ranges(&self) -> Vec<SourceRange<'tu>> {
        iter!(
            clang_getDiagnosticNumRanges(self.ptr),
            clang_getDiagnosticRange(self.ptr),
        ).map(|r| SourceRange::from_raw(r, self.tu)).collect()
    }

    /// Returns the fix-its for this diagnostic.
    pub fn get_fix_its(&self) -> Vec<FixIt<'tu>> {
        unsafe {
            (0..ffi::clang_getDiagnosticNumFixIts(self.ptr)).map(|i| {
                let mut range = mem::uninitialized();
                let fixit = ffi::clang_getDiagnosticFixIt(self.ptr, i, &mut range);
                let string = utility::to_string(fixit);
                let range = SourceRange::from_raw(range, self.tu);
                if string.is_empty() {
                    FixIt::Deletion(range)
                } else if range.get_start() == range.get_end() {
                    FixIt::Insertion(range.get_start(), string)
                } else {
                    FixIt::Replacement(range, string)
                }
            }).collect()
        }
    }

    /// Returns the child diagnostics of this diagnostic.
    pub fn get_children(&self) -> Vec<Diagnostic> {
        let raw = unsafe { ffi::clang_getChildDiagnostics(self.ptr) };
        iter!(
            clang_getNumDiagnosticsInSet(raw),
            clang_getDiagnosticInSet(raw),
        ).map(|d| Diagnostic::from_ptr(d, self.tu)).collect()
    }

    /// Returns a diagnostic formatter that builds a formatted string from this diagnostic.
    pub fn formatter(&self) -> DiagnosticFormatter<'tu> {
        DiagnosticFormatter::new(*self)
    }
}

impl<'tu> fmt::Debug for Diagnostic<'tu> {
    fn fmt(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.debug_struct("Diagnostic")
            .field("location", &self.get_location())
            .field("severity", &self.get_severity())
            .field("text", &self.get_text())
            .finish()
    }
}

impl<'tu> fmt::Display for Diagnostic<'tu> {
    fn fmt(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        write!(formatter, "{}", DiagnosticFormatter::new(*self).format())
    }
}

// DiagnosticFormatter ___________________________

builder! {
    /// Builds formatted strings from diagnostics.
    builder DiagnosticFormatter: CXDiagnosticDisplayOptions {
        diagnostic: Diagnostic<'tu>;
    OPTIONS:
        /// Sets whether the diagnostic text will be prefixed by the file and line of the source
        /// location the diagnostic indicates. This prefix may also contain column and/or source
        /// range information.
        pub source_location: CXDiagnostic_DisplaySourceLocation,
        /// Sets whether the column will be included in the source location prefix.
        pub column: CXDiagnostic_DisplayColumn,
        /// Sets whether the source ranges will be included to the source location prefix.
        pub source_ranges: CXDiagnostic_DisplaySourceRanges,
        /// Sets whether the option associated with the diagnostic (e.g., `-Wconversion`) will be
        /// placed in brackets after the diagnostic text if there is such an option.
        pub option: CXDiagnostic_DisplayOption,
        /// Sets whether the category number associated with the diagnostic will be placed in
        /// brackets after the diagnostic text if there is such a category number.
        pub category_id: CXDiagnostic_DisplayCategoryId,
        /// Sets whether the category name associated with the diagnostic will be placed in brackets
        /// after the diagnostic text if there is such a category name.
        pub category_name: CXDiagnostic_DisplayCategoryName,
    }
}

impl<'tu> DiagnosticFormatter<'tu> {
    //- Constructors -----------------------------

    fn new(diagnostic: Diagnostic<'tu>) -> DiagnosticFormatter<'tu> {
        let flags = unsafe { ffi::clang_defaultDiagnosticDisplayOptions() };
        DiagnosticFormatter { diagnostic: diagnostic, flags: flags }
    }

    //- Accessors --------------------------------

    /// Returns a formatted string.
    pub fn format(&self) -> String {
        let ptr = self.diagnostic.ptr;
        unsafe { utility::to_string(ffi::clang_formatDiagnostic(ptr, self.flags)) }
    }
}