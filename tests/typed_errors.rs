//! The readers that once returned `String` errors refuse with matchable variants that name the field and the offset
//! of the bad bytes, from bodies laid out here by hand.
use anvil_formats::controller::{self, Controller};
use anvil_formats::fields::BodyError;
use anvil_formats::gidyn::{self, StoredGiDyn};
use anvil_formats::impostor::{self, StoredImpostor};
use anvil_formats::light::{self, StoredSpotLight};
use anvil_formats::look::{self, AMBIANCE_PRESET, FORECAST_PRESET, StoredLook};
use anvil_formats::shaunwhite::shaders;
use anvil_formats::sky::{self, StoredSky};

fn put(b: &mut [u8], at: usize, bytes: &[u8]) {
    b.get_mut(at..at + bytes.len())
        .expect("inside the body")
        .copy_from_slice(bytes);
}

#[test]
fn a_spot_light_refuses_by_length_and_by_the_float_at_its_offset() {
    let e = StoredSpotLight::parse(&[0; 10]).expect_err("refused");
    assert!(matches!(
        e,
        light::Error::Body(BodyError::Value {
            what: "SpotLight length",
            at: 0,
            found: 10,
            ..
        })
    ));
    let mut body = vec![0; 328];
    put(&mut body, 34, &0xA797_67EDu32.to_le_bytes());
    put(&mut body, 320, &1.0f32.to_le_bytes());
    assert!(StoredSpotLight::parse(&body).is_ok());
    put(&mut body, 54, &f32::NAN.to_bits().to_le_bytes());
    let e = StoredSpotLight::parse(&body).expect_err("refused");
    assert!(matches!(
        e,
        light::Error::Body(BodyError::Value {
            what: "intensity",
            at: 54,
            ..
        })
    ));
    assert!(e.to_string().contains("a finite number"), "{e}");
}

#[test]
fn a_look_preset_names_the_field_out_of_range_and_the_profile_it_does_not_read() {
    let mut body = vec![1, 0];
    body.extend(17u64.to_le_bytes());
    body.extend([3, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
    body.extend(30f32.to_le_bytes());
    body.extend(135f32.to_le_bytes());
    body.push(0);
    let e = StoredLook::parse(AMBIANCE_PRESET, &body).expect_err("refused");
    assert!(matches!(
        e,
        look::Error::Body(BodyError::Value {
            what: "hour",
            at: 20,
            ..
        })
    ));
    let e = StoredLook::parse(FORECAST_PRESET, &body).expect_err("refused");
    assert!(matches!(e, look::Error::Unsupported { length: 29, .. }));
}

#[test]
fn an_impostor_names_the_card_count_it_refuses() {
    let mut body = vec![0; 199 + 108];
    for index in 0..4 {
        put(&mut body, 71 + index * 10, &[3]);
    }
    put(&mut body, 119, &9u32.to_le_bytes());
    let e = StoredImpostor::parse(&body).expect_err("refused");
    assert!(matches!(
        e,
        impostor::Error::Body(BodyError::Value {
            what: "side card count",
            at: 119,
            found: 9,
            ..
        })
    ));
}

#[test]
fn a_gi_resource_names_the_offset_of_its_bad_count() {
    let mut body = vec![1];
    body.extend(7u64.to_le_bytes());
    body.extend(27u32.to_le_bytes());
    body.extend(2u32.to_le_bytes());
    let e = StoredGiDyn::parse(&body).expect_err("refused");
    assert!(matches!(
        e,
        gidyn::Error::Body(BodyError::Value {
            what: "volume count",
            at: 13,
            found: 2,
            ..
        })
    ));
}

#[test]
fn a_controller_names_its_entry_count_and_a_truncation() {
    let mut body = vec![1];
    body.extend(9000u32.to_le_bytes());
    let e = Controller::parse(&body).expect_err("refused");
    assert!(matches!(
        e,
        controller::Error::Body(BodyError::Value {
            what: "entry count",
            at: 1,
            found: 9000,
            ..
        })
    ));
    let e = Controller::parse(&[1, 1, 0, 0, 0]).expect_err("refused");
    assert!(matches!(e, controller::Error::Body(BodyError::Short(_))));
}

#[test]
fn a_sky_error_names_the_absolute_offset_inside_a_keyframe() {
    let e = StoredSky::parse(&[0; 10]).expect_err("refused");
    assert!(matches!(
        e,
        sky::Error::Body(BodyError::Value {
            what: "LayeredSky length",
            ..
        })
    ));
    // the one-keyframe profile: the keyframe starts at 572 and holds its class at +8, so the file offset is 580
    let mut body = vec![0; 143_994];
    put(&mut body, 568, &1u32.to_le_bytes());
    let e = StoredSky::parse(&body).expect_err("refused");
    assert!(
        matches!(
            e,
            sky::Error::Body(BodyError::Value {
                what: "keyframe class",
                at: 580,
                ..
            })
        ),
        "{e:?}"
    );
    let e = sky::ambiance(&[0; 3]).expect_err("refused");
    assert!(matches!(
        e,
        sky::Error::Body(BodyError::Value {
            what: "WorldAmbiance length",
            ..
        })
    ));
}

/// An SDB2 with no sources and one kind-4 record whose payload offset points into the record table.
#[test]
fn an_sdb2_names_the_offset_field_that_is_wrong() {
    let mut b = Vec::new();
    b.extend(b"SDB2");
    for word in [39u32, 0, 1, 24, 24] {
        b.extend(word.to_le_bytes());
    }
    b.extend(4u32.to_le_bytes()); // kind
    b.extend(0u64.to_le_bytes()); // permutation
    b.extend([0; 16]); // dependency mask
    b.extend(0u32.to_le_bytes()); // payload offset: inside the table
    b.extend(0u32.to_le_bytes()); // payload length
    let e = shaders::read(&b).expect_err("refused");
    // the record starts at 24 and its offset field is its 28th byte
    assert!(
        matches!(
            e,
            shaders::Error::Invalid {
                at: 52,
                field: "first payload offset",
                ..
            }
        ),
        "{e}"
    );
}
