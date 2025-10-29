export NO_PROXY="127.0.0.1,localhost,0.0.0.0"
export no_proxy="127.0.0.1,localhost,0.0.0.0"

arcium clean
arcium build

rm -rf node_modules
rm -f yarn.lock 
yarn install

arcium test