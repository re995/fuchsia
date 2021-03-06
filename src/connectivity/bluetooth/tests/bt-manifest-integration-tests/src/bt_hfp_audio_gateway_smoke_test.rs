// Copyright 2021 The Fuchsia Authors. All rights reserved.
// Use of this source code is governed by a BSD-style license that can be
// found in the LICENSE file.

use {
    anyhow::Error,
    bt_manifest_integration_lib::mock_component,
    fidl_fuchsia_bluetooth_bredr::{ProfileMarker, ProfileRequest},
    fidl_fuchsia_bluetooth_hfp::{HfpMarker, HfpProxy},
    fidl_fuchsia_bluetooth_hfp_test::{HfpTestMarker, HfpTestProxy},
    fuchsia_async as fasync,
    fuchsia_component_test::{
        builder::{ComponentSource, RealmBuilder, RouteEndpoint},
        mock::{Mock, MockHandles},
    },
    futures::{channel::mpsc, SinkExt, StreamExt},
    log::info,
};

/// HFP Audio Gateway component URL.
const HFP_AG_URL: &str =
    "fuchsia-pkg://fuchsia.com/bt-hfp-audio-gateway-smoke-test#meta/bt-hfp-audio-gateway.cm";

/// Local name of the HFP component under test.
const HFP_MONIKER: &str = "hfp";
/// Local name of the mock that is providing the `bredr.Profile` service.
const FAKE_PROFILE_MONIKER: &str = "fake-profile";
/// Local name of the fake HFP client that is connecting to `HFP_MONIKER`s services.
const HFP_CLIENT_MONIKER: &str = "fake-hfp-client";

/// The different events generated by this test.
/// Note: In order to prevent the component under test from terminating, any FIDL request or
/// Proxy is preserved.
#[derive(Debug)]
enum Event {
    /// A BR/EDR Profile event.
    Profile(Option<ProfileRequest>),
    /// HFP service client connection.
    Hfp(Option<HfpProxy>),
    /// HFP Test service client connection.
    HfpTest(Option<HfpTestProxy>),
}

impl From<ProfileRequest> for Event {
    fn from(src: ProfileRequest) -> Self {
        Self::Profile(Some(src))
    }
}

/// Represents a fake HFP client that requests the `Hfp` and `HfpTest` services.
async fn mock_hfp_client(
    mut sender: mpsc::Sender<Event>,
    handles: MockHandles,
) -> Result<(), Error> {
    let hfp_svc = handles.connect_to_service::<HfpMarker>()?;
    sender.send(Event::Hfp(Some(hfp_svc))).await.expect("failed sending ack to test");

    let hfp_test_svc = handles.connect_to_service::<HfpTestMarker>()?;
    sender.send(Event::HfpTest(Some(hfp_test_svc))).await.expect("failed sending ack to test");
    Ok(())
}

/// Tests that the v2 HFP Audio Gateway component has the correct topology and verifies that
/// it provides and connects to the expected services.
#[fasync::run_singlethreaded(test)]
async fn hfp_audio_gateway_v2_capability_routing() {
    fuchsia_syslog::init().unwrap();
    info!("Starting HFP Audio Gateway v2 smoke test...");

    let (sender, mut receiver) = mpsc::channel(4);
    let profile_tx = sender.clone();
    let fake_client_tx = sender.clone();

    let mut builder = RealmBuilder::new().await.expect("Failed to create test realm builder");
    // The v2 component under test.
    builder
        .add_eager_component(HFP_MONIKER, ComponentSource::url(HFP_AG_URL.to_string()))
        .await
        .expect("Failed adding HFP-AG to topology");
    // Mock Profile component to receive `bredr.Profile` requests.
    builder
        .add_component(
            FAKE_PROFILE_MONIKER,
            ComponentSource::Mock(Mock::new({
                move |mock_handles: MockHandles| {
                    let sender = profile_tx.clone();
                    Box::pin(mock_component::<ProfileMarker, _>(sender, mock_handles))
                }
            })),
        )
        .await
        .expect("Failed adding profile mock to topology");
    // Mock HFP-AG client that will request the `Hfp` and `HfpTest` services
    // which are provided by `bt-hfp-audio-gateway.cml`.
    builder
        .add_eager_component(
            HFP_CLIENT_MONIKER,
            ComponentSource::Mock(Mock::new({
                move |mock_handles: MockHandles| {
                    let sender = fake_client_tx.clone();
                    Box::pin(mock_hfp_client(sender, mock_handles))
                }
            })),
        )
        .await
        .expect("Failed adding hfp client mock to topology");

    // Set up capabilities.
    builder
        .add_protocol_route::<HfpMarker>(
            RouteEndpoint::component(HFP_MONIKER),
            vec![RouteEndpoint::component(HFP_CLIENT_MONIKER)],
        )
        .expect("Failed adding route for Hfp service")
        .add_protocol_route::<HfpTestMarker>(
            RouteEndpoint::component(HFP_MONIKER),
            vec![RouteEndpoint::component(HFP_CLIENT_MONIKER)],
        )
        .expect("Failed adding route for HfpTest service")
        .add_protocol_route::<ProfileMarker>(
            RouteEndpoint::component(FAKE_PROFILE_MONIKER),
            vec![RouteEndpoint::component(HFP_MONIKER)],
        )
        .expect("Failed adding route for Profile service")
        .add_protocol_route::<fidl_fuchsia_logger::LogSinkMarker>(
            RouteEndpoint::AboveRoot,
            vec![
                RouteEndpoint::component(HFP_MONIKER),
                RouteEndpoint::component(FAKE_PROFILE_MONIKER),
                RouteEndpoint::component(HFP_CLIENT_MONIKER),
            ],
        )
        .expect("Failed adding LogSink route to test components");
    let _test_topology = builder.build().create().await.unwrap();

    // If the routing is correctly configured, we expect four events:
    //   1. `hfp-audio-gateway` connecting to the Profile service.
    //     a. Making a request to Advertise.
    //     b. Making a request to Search.
    //   2. `fake-hfp-client` connecting to the Hfp service which is provided by
    //      `hfp-audio-gateway`.
    //   3. `fake-hfp-client` connecting to the HfpTest service which is provided by
    //      `hfp-audio-gateway`.
    let mut events = Vec::new();
    let expected_number_of_events = 4;
    for i in 0..expected_number_of_events {
        let msg = format!("Unexpected error waiting for {:?} event", i);
        events.push(receiver.next().await.expect(&msg));
    }
    assert_eq!(events.len(), 4);

    assert_eq!(
        events
            .iter()
            .filter(|&d| std::mem::discriminant(d) == std::mem::discriminant(&Event::Profile(None)))
            .count(),
        2
    );
    assert_eq!(
        events
            .iter()
            .filter(|&d| std::mem::discriminant(d) == std::mem::discriminant(&Event::Hfp(None)))
            .count(),
        1
    );
    assert_eq!(
        events
            .iter()
            .filter(|&d| std::mem::discriminant(d) == std::mem::discriminant(&Event::HfpTest(None)))
            .count(),
        1
    );

    info!("Finished HFP Audio Gateway smoke test");
}
