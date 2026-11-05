use super::DeliveryChannel;

#[test]
fn each_channel_names_its_own_mechanism_in_the_step_label() {
    let labels = [
        (DeliveryChannel::Xtest, "XTEST"),
        (DeliveryChannel::RemoteDesktop, "RemoteDesktop"),
        (DeliveryChannel::VirtualKeyboard, "VirtualKeyboard"),
    ];
    for (channel, name) in labels {
        assert_eq!(channel.step_label("type_text"), format!("{name}.type_text"));
        assert_eq!(channel.step_label("press_key"), format!("{name}.press_key"));
    }
}
