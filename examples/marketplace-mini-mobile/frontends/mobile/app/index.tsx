import { SafeAreaView, Text } from "react-native";

export default function Home() {
  return (
    <SafeAreaView style={{ flex: 1, alignItems: "center", justifyContent: "center" }}>
      <Text>Lazurite mobile scaffold. Add a `surface ... mobile` and run `lazuli generate ts`.</Text>
    </SafeAreaView>
  );
}
