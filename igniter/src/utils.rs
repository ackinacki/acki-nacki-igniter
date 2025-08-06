use std::collections::HashMap;
use std::vec;

use serde::Deserialize;
use serde::Serialize;

pub trait ContainsVec<T: HasTimestampAndId> {
    fn get_mut_vec(&mut self) -> &mut Vec<T>;
    fn is_empty(&self) -> bool;
    fn get_pk(&self) -> String;
}

pub trait HasTimestampAndId {
    fn get_id(&self) -> String;
    fn get_timestamp(&self) -> u64;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RevokedLicense {
    pub license_id: String,
    pub provider_pubkey: String,
    pub timestamp: u64,
}

pub fn remove_with_outdated_timestamps<S, T>(mut data: Vec<S>) -> (Vec<S>, Vec<RevokedLicense>)
where
    T: HasTimestampAndId + std::fmt::Debug,
    S: ContainsVec<T>,
{
    let mut max_map: HashMap<String, u64> = HashMap::new();
    for inner in &mut data {
        for item in inner.get_mut_vec() {
            max_map
                .entry(item.get_id())
                .and_modify(|ts| {
                    if item.get_timestamp() > *ts {
                        *ts = item.get_timestamp();
                    }
                })
                .or_insert(item.get_timestamp());
        }
    }

    let mut revoked_licenses = vec![];
    for inner in &mut data {
        let pk = inner.get_pk();
        inner.get_mut_vec().retain(|item| {
            max_map.get(&item.get_id()).is_some_and(|&max_ts| {
                if item.get_timestamp() == max_ts {
                    return true;
                }
                revoked_licenses.push(RevokedLicense {
                    license_id: item.get_id(),
                    provider_pubkey: pk.clone(),
                    timestamp: item.get_timestamp(),
                });
                false // remove the item if its timestamp is outdated
            })
        });
    }

    // remove duplicates
    for inner in &mut data {
        let mut seen = HashMap::new();
        inner.get_mut_vec().retain(|item| {
            let id = item.get_id();
            if let std::collections::hash_map::Entry::Vacant(e) = seen.entry(id) {
                e.insert(true);
                true // keep the item
            } else {
                false
            }
        });
    }
    // Remove empty inner vectors
    data.retain(|inner| !inner.is_empty());

    (data, revoked_licenses)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug)]
    struct InnerStruct {
        id: String,
        ts: u64,
    }
    impl HasTimestampAndId for InnerStruct {
        fn get_id(&self) -> String {
            self.id.clone()
        }

        fn get_timestamp(&self) -> u64 {
            self.ts
        }
    }

    #[derive(Debug)]
    struct OuterStruct {
        index: usize,
        inner: Vec<InnerStruct>,
    }
    impl ContainsVec<InnerStruct> for OuterStruct {
        fn get_mut_vec(&mut self) -> &mut Vec<InnerStruct> {
            &mut self.inner
        }

        fn is_empty(&self) -> bool {
            self.inner.is_empty()
        }

        fn get_pk(&self) -> String {
            format!("pk_{}", self.index)
        }
    }

    #[test]
    fn test_remove_with_outdated_timestamps_1() {
        let data: Vec<OuterStruct> = vec![];
        let expected_output: Vec<OuterStruct> = vec![];
        let expected_problem: Vec<String> = vec![];
        let (output, problem) = remove_with_outdated_timestamps(data);
        assert_eq!(format!("{output:?}"), format!("{expected_output:?}"));
        assert_eq!(format!("{problem:?}"), format!("{expected_problem:?}"));
    }

    #[test]
    fn test_remove_with_outdated_timestamps_2() {
        let data: Vec<OuterStruct> = vec![OuterStruct {
            index: 0,
            inner: vec![
                InnerStruct { id: "a".into(), ts: 10 }, // retained
                InnerStruct { id: "b".into(), ts: 20 }, // removed
                InnerStruct { id: "b".into(), ts: 40 }, // retained
                InnerStruct { id: "a".into(), ts: 10 }, // removed as duplicate
            ],
        }];
        let expected_output: Vec<OuterStruct> = vec![OuterStruct {
            index: 0,
            inner: vec![
                InnerStruct { id: "a".into(), ts: 10 },
                InnerStruct { id: "b".into(), ts: 40 },
            ],
        }];

        let expected_problem = vec![RevokedLicense {
            provider_pubkey: "pk_0".to_string(),
            license_id: "b".to_string(),
            timestamp: 20,
        }];
        let (output, problem) = remove_with_outdated_timestamps(data);

        assert_eq!(format!("{output:?}"), format!("{expected_output:?}"));
        assert_eq!(format!("{problem:?}"), format!("{expected_problem:?}"));
    }
    #[test]
    fn test_remove_with_outdated_timestamps_3() {
        let data = vec![
            OuterStruct {
                index: 0,
                inner: vec![
                    InnerStruct { id: "a".into(), ts: 10 },
                    InnerStruct { id: "b".into(), ts: 20 },
                ],
            },
            OuterStruct {
                index: 1,
                inner: vec![
                    InnerStruct { id: "a".into(), ts: 15 },
                    InnerStruct { id: "c".into(), ts: 30 },
                ],
            },
            OuterStruct { index: 2, inner: vec![InnerStruct { id: "b".into(), ts: 5 }] },
        ];

        let expected_output = vec![
            OuterStruct {
                index: 0,
                inner: vec![
                    //
                    InnerStruct { id: "b".into(), ts: 20 },
                ],
            },
            OuterStruct {
                index: 1,
                inner: vec![
                    InnerStruct { id: "a".into(), ts: 15 },
                    InnerStruct { id: "c".into(), ts: 30 },
                ],
            },
        ];

        let expected_problem = [
            RevokedLicense {
                license_id: "a".to_string(),
                provider_pubkey: "pk_0".to_string(),
                timestamp: 10,
            },
            RevokedLicense {
                license_id: "b".to_string(),
                provider_pubkey: "pk_2".to_string(),
                timestamp: 5,
            },
        ];

        let (output, problem) = remove_with_outdated_timestamps(data);

        assert_eq!(format!("{output:?}"), format!("{expected_output:?}"));
        assert_eq!(format!("{problem:?}"), format!("{expected_problem:?}"));
    }
}
