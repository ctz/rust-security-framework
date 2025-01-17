//! OSX specific extensions to certificate functionality.

use core_foundation::array::{CFArray, CFArrayIterator};
use core_foundation::base::TCFType;
use core_foundation::base::ToVoid;
use core_foundation::dictionary::CFDictionary;
use core_foundation::error::CFError;
use core_foundation::string::CFString;
use std::os::raw::c_void;
use security_framework_sys::certificate::*;
use std::ptr;

use base::Error;
use certificate::SecCertificate;
use cvt;
use key::SecKey;
use os::macos::certificate_oids::CertificateOid;

/// An extension trait adding OSX specific functionality to `SecCertificate`.
pub trait SecCertificateExt {
    /// Returns the common name associated with the certificate.
    fn common_name(&self) -> Result<String, Error>;

    /// Returns the public key associated with the certificate.
    fn public_key(&self) -> Result<SecKey, Error>;

    /// Returns the set of properties associated with the certificate.
    ///
    /// The `keys` argument can optionally be used to filter the properties loaded to an explicit
    /// subset.
    fn properties(&self, keys: Option<&[CertificateOid]>)
        -> Result<CertificateProperties, CFError>;
}

impl SecCertificateExt for SecCertificate {
    fn common_name(&self) -> Result<String, Error> {
        unsafe {
            let mut string = ptr::null();
            cvt(SecCertificateCopyCommonName(
                self.as_concrete_TypeRef(),
                &mut string,
            ))?;
            Ok(CFString::wrap_under_create_rule(string).to_string())
        }
    }

    fn public_key(&self) -> Result<SecKey, Error> {
        unsafe {
            let mut key = ptr::null_mut();
            cvt(SecCertificateCopyPublicKey(
                self.as_concrete_TypeRef(),
                &mut key,
            ))?;
            Ok(SecKey::wrap_under_create_rule(key))
        }
    }

    fn properties(
        &self,
        keys: Option<&[CertificateOid]>,
    ) -> Result<CertificateProperties, CFError> {
        unsafe {
            let keys = keys.map(|oids| {
                let oids = oids.iter().map(|oid| oid.to_str()).collect::<Vec<_>>();
                CFArray::from_CFTypes(&oids)
            });

            let keys = match keys {
                Some(ref keys) => keys.as_concrete_TypeRef(),
                None => ptr::null_mut(),
            };

            let mut error = ptr::null_mut();

            let dictionary = SecCertificateCopyValues(self.as_concrete_TypeRef(), keys, &mut error);

            if error.is_null() {
                Ok(CertificateProperties(CFDictionary::wrap_under_create_rule(
                    dictionary,
                )))
            } else {
                Err(CFError::wrap_under_create_rule(error))
            }
        }
    }
}

/// Properties associated with a certificate.
pub struct CertificateProperties(CFDictionary);

impl CertificateProperties {
    /// Retrieves a specific property identified by its OID.
    pub fn get(&self, oid: CertificateOid) -> Option<CertificateProperty> {
        unsafe {
            self.0.find(oid.as_ptr() as *const c_void).map(|value| {
                CertificateProperty(CFDictionary::wrap_under_get_rule(*value as *mut _))
            })
        }
    }
}

/// A property associated with a certificate.
pub struct CertificateProperty(CFDictionary);

impl CertificateProperty {
    /// Returns the label of this property.
    pub fn label(&self) -> CFString {
        unsafe {
            CFString::wrap_under_get_rule(*self.0.get(kSecPropertyKeyLabel.to_void()) as *const _)
        }
    }

    /// Returns an enum of the underlying data for this property.
    pub fn get(&self) -> PropertyType {
        unsafe {
            let type_ = CFString::wrap_under_get_rule(
                *self.0.get(kSecPropertyKeyType.to_void()) as *mut _,
            );
            let value = self.0.get(kSecPropertyKeyValue.to_void());

            if type_ == CFString::wrap_under_get_rule(kSecPropertyTypeSection) {
                PropertyType::Section(PropertySection(CFArray::wrap_under_get_rule(
                    *value as *const _,
                )))
            } else if type_ == CFString::wrap_under_get_rule(kSecPropertyTypeString) {
                PropertyType::String(CFString::wrap_under_get_rule(*value as *const _))
            } else {
                PropertyType::__Unknown
            }
        }
    }
}

/// A "section" property.
///
/// Sections are sequences of other properties.
pub struct PropertySection(CFArray<CFDictionary>);

impl PropertySection {
    /// Returns an iterator over the properties in this section.
    pub fn iter<'a>(&'a self) -> PropertySectionIter<'a> {
        PropertySectionIter(self.0.iter())
    }
}

impl<'a> IntoIterator for &'a PropertySection {
    type IntoIter = PropertySectionIter<'a>;
    type Item = CertificateProperty;

    fn into_iter(self) -> PropertySectionIter<'a> {
        self.iter()
    }
}

/// An iterator over the properties in a section.
pub struct PropertySectionIter<'a>(CFArrayIterator<'a, CFDictionary>);

impl<'a> Iterator for PropertySectionIter<'a> {
    type Item = CertificateProperty;

    fn next(&mut self) -> Option<CertificateProperty> {
        self.0.next().map(|t| CertificateProperty(t.clone()))
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.0.size_hint()
    }
}

/// An enum of the various types of properties.
pub enum PropertyType {
    /// A section.
    Section(PropertySection),
    /// A string.
    String(CFString),
    #[doc(hidden)]
    __Unknown,
}

#[cfg(test)]
mod test {
    use super::*;
    use os::macos::certificate_oids::CertificateOid;
    use std::collections::HashMap;
    use test::certificate;

    #[test]
    fn common_name() {
        let certificate = certificate();
        assert_eq!("foobar.com", p!(certificate.common_name()).to_string());
    }

    #[test]
    fn public_key() {
        let certificate = certificate();
        p!(certificate.public_key());
    }

    #[test]
    fn signature_algorithm() {
        let certificate = certificate();
        let properties = certificate
            .properties(Some(&[CertificateOid::x509_v1_signature_algorithm()]))
            .unwrap();
        let value = properties
            .get(CertificateOid::x509_v1_signature_algorithm())
            .unwrap();
        let section = match value.get() {
            PropertyType::Section(section) => section,
            _ => panic!(),
        };
        let properties = section
            .iter()
            .map(|p| (p.label().to_string(), p.get()))
            .collect::<HashMap<_, _>>();
        let algorithm = match properties["Algorithm"] {
            PropertyType::String(ref s) => s.to_string(),
            _ => panic!(),
        };
        assert_eq!(algorithm, "1.2.840.113549.1.1.5");
    }
}
