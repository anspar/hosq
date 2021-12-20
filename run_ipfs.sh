ipfs_staging="$PWD/staging"
ipfs_data="$PWD/data"

if [ ! -d "$ipfs_staging" ] 
then
    echo "Creating dir $ipfs_staging"
    mkdir $ipfs_staging 
fi

if [ ! -d "$ipfs_data" ]
then
    echo "Creating dir $ipfs_data"
    mkdir $ipfs_data
fi

docker rm -f ipfs_host
docker run -d --name ipfs_host -v $ipfs_staging:/export -v $ipfs_data:/data/ipfs -p 4001:4001 -p 8080:8080 -p 5001:5001 ipfs/go-ipfs:latest

